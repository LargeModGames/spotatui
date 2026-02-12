pub mod audio_analysis;
pub mod help;
pub mod settings;
pub mod util;
use super::{
  app::{
    ActiveBlock, AlbumTableContext, App, ArtistBlock, EpisodeTableContext, RecommendationsContext,
    RouteId, SearchResultBlock, LIBRARY_OPTIONS,
  },
  banner::BANNER,
};
use colorgrad::{self, Gradient};
use help::get_help_docs;
use ratatui::{
  layout::{Alignment, Constraint, Direction, Layout, Rect},
  style::{Color, Modifier, Style},
  text::{Line, Span, Text},
  widgets::{
    canvas::Canvas, Block, BorderType, Borders, Clear, LineGauge, List, ListItem, ListState,
    Paragraph, Row, Table, Wrap,
  },
  Frame,
};
use rspotify::model::enums::RepeatState;
use rspotify::model::show::ResumePoint;
use rspotify::model::PlayableItem;
use rspotify::prelude::Id;
use std::sync::{Mutex, OnceLock};
use unicode_width::UnicodeWidthStr;
use util::{
  create_artist_string, display_track_progress, get_artist_highlight_state, get_color,
  get_percentage_width, get_search_results_highlight_state, get_track_progress_percentage,
  millis_to_minutes, BASIC_VIEW_HEIGHT, SMALL_TERMINAL_WIDTH,
};

pub enum TableId {
  Album,
  AlbumList,
  Artist,
  Podcast,
  Song,
  RecentlyPlayed,
  PodcastEpisodes,
}

#[derive(Default, PartialEq)]
pub enum ColumnId {
  #[default]
  None,
  Title,
  Liked,
}

pub struct TableHeader<'a> {
  id: TableId,
  items: Vec<TableHeaderItem<'a>>,
}

impl TableHeader<'_> {
  pub fn get_index(&self, id: ColumnId) -> Option<usize> {
    self.items.iter().position(|item| item.id == id)
  }
}

#[derive(Default)]
pub struct TableHeaderItem<'a> {
  id: ColumnId,
  text: &'a str,
  width: u16,
}

pub struct TableItem {
  id: String,
  format: Vec<String>,
}

#[derive(Clone, PartialEq)]
struct HomeThemeKey {
  banner: Color,
  active: Color,
  hovered: Color,
  text: Color,
  hint: Color,
  inactive: Color,
  changelog_width: u16,
}

impl HomeThemeKey {
  fn from_theme(theme: &crate::user_config::Theme, changelog_width: u16) -> Self {
    Self {
      banner: theme.banner,
      active: theme.active,
      hovered: theme.hovered,
      text: theme.text,
      hint: theme.hint,
      inactive: theme.inactive,
      changelog_width,
    }
  }
}

struct HomeCache {
  theme_key: HomeThemeKey,
  gradient_lines: Vec<Line<'static>>,
  changelog_lines: Vec<Line<'static>>,
}

static HOME_CACHE: OnceLock<Mutex<HomeCache>> = OnceLock::new();
static CLEAN_CHANGELOG: OnceLock<String> = OnceLock::new();

pub fn draw_help_menu(f: &mut Frame<'_>, app: &App) {
  let [area] = f
    .area()
    .layout(&Layout::vertical([Constraint::Percentage(100)]).margin(2));

  // Create a one-column table to avoid flickering due to non-determinism when
  // resolving constraints on widths of table columns.
  let format_row =
    |r: Vec<String>| -> Vec<String> { vec![format!("{:50}{:40}{:20}", r[0], r[1], r[2])] };

  let help_menu_style = app.user_config.theme.base_style();
  let header = ["Description", "Event", "Context"];
  let header = format_row(header.iter().map(|s| s.to_string()).collect());

  let help_docs = get_help_docs(&app.user_config.keys);
  let help_docs = help_docs
    .into_iter()
    .map(format_row)
    .collect::<Vec<Vec<String>>>();
  let help_docs = &help_docs[app.help_menu_offset as usize..];

  let rows = help_docs
    .iter()
    .map(|item| Row::new(item.clone()).style(help_menu_style));

  let help_menu = Table::new(rows, &[Constraint::Percentage(100)])
    .header(Row::new(header))
    .block(
      Block::default()
        .borders(Borders::ALL)
        .style(help_menu_style)
        .title(Span::styled(
          "Help (press <Esc> to go back)",
          help_menu_style,
        ))
        .border_style(help_menu_style),
    )
    .style(help_menu_style);
  f.render_widget(help_menu, area);
}

pub fn draw_input_and_help_box(f: &mut Frame<'_>, app: &App, layout_chunk: Rect) {
  // Check for the width and change the constraints accordingly
  let constraints = if app.size.width >= SMALL_TERMINAL_WIDTH
    && !app.user_config.behavior.enforce_wide_search_bar
  {
    [Constraint::Percentage(65), Constraint::Percentage(35)]
  } else {
    [Constraint::Percentage(90), Constraint::Percentage(10)]
  };

  let [input_area, help_area] = layout_chunk.layout(&Layout::horizontal(constraints));

  let current_route = app.get_current_route();

  let highlight_state = (
    current_route.active_block == ActiveBlock::Input,
    current_route.hovered_block == ActiveBlock::Input,
  );

  let show_loading = app.is_loading && app.user_config.behavior.show_loading_indicator;
  let border_type = if show_loading {
    BorderType::Double
  } else {
    BorderType::Rounded
  };

  let input_string: String = app.input.iter().collect();
  let lines = Text::from(input_string.clone());
  // Compute horizontal scroll so the cursor stays visible within the input box.
  // inner width = total width - 2 (for left and right borders)
  let inner_width = input_area.width.saturating_sub(2);
  let scroll_offset = if inner_width > 0 && app.input_cursor_position >= inner_width {
    app.input_cursor_position - inner_width + 1
  } else {
    0
  };
  app.input_scroll_offset.set(scroll_offset);

  let input = Paragraph::new(lines)
    .scroll((0, scroll_offset))
    .block(
    Block::default()
      .borders(Borders::ALL)
      .border_type(border_type)
      .title(Span::styled(
        "Search",
        get_color(highlight_state, app.user_config.theme),
      ))
      .style(app.user_config.theme.base_style())
      .border_style(get_color(highlight_state, app.user_config.theme)),
  );
  f.render_widget(input, input_area);

  let help_block_text = if show_loading {
    (app.user_config.theme.hint, "Loading...")
  } else {
    (app.user_config.theme.inactive, "Type ?")
  };

  let block = Block::default()
    .title(Span::styled("Help", Style::default().fg(help_block_text.0)))
    .borders(Borders::ALL)
    .border_type(BorderType::Rounded)
    .border_style(Style::default().fg(help_block_text.0));

  let lines = Text::from(help_block_text.1);
  let help = Paragraph::new(lines).block(block).style(
    Style::default()
      .fg(help_block_text.0)
      .bg(app.user_config.theme.background),
  );
  f.render_widget(help, help_area);
}

pub fn draw_main_layout(f: &mut Frame<'_>, app: &App) {
  let margin = util::get_main_layout_margin(app);
  // Responsive layout: new one kicks in at width 150 or higher
  if app.size.width >= SMALL_TERMINAL_WIDTH && !app.user_config.behavior.enforce_wide_search_bar {
    let [routes_area, playbar_area] = f
      .area()
      .layout(&Layout::vertical([Constraint::Min(1), Constraint::Length(6)]).margin(margin));

    // Nested main block with potential routes
    draw_routes(f, app, routes_area);

    // Currently playing
    draw_playbar(f, app, playbar_area);
  } else {
    let [input_area, routes_area, playbar_area] = f.area().layout(
      &Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(1),
        Constraint::Length(6),
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
  let [user_area, content_area] = layout_chunk.layout(&Layout::horizontal([
    Constraint::Percentage(20),
    Constraint::Percentage(80),
  ]));

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
    RouteId::Error => {} // This is handled as a "full screen" route in main.rs
    RouteId::SelectedDevice => {} // This is handled as a "full screen" route in main.rs
    RouteId::Analysis => {} // This is handled as a "full screen" route in main.rs
    RouteId::BasicView => {} // This is handled as a "full screen" route in main.rs
    RouteId::Dialog => {} // This is handled in the draw_dialog function in mod.rs
    RouteId::UpdatePrompt => {} // This is handled as a "full screen" route in main.rs
    RouteId::Settings => {} // This is handled as a "full screen" route in main.rs
    RouteId::HelpMenu => {} // This is handled as a "full screen" route in main.rs
  };
}

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
        crate::app::PlaylistFolderItem::Folder(folder) => {
          if folder.name.starts_with('\u{2190}') {
            // Back entry (already has arrow prefix)
            folder.name.clone()
          } else {
            format!("\u{1F4C1} {}", folder.name)
          }
        }
        crate::app::PlaylistFolderItem::Playlist { index, .. } => app
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

  draw_selectable_list(
    f,
    app,
    layout_chunk,
    "Playlists",
    &playlist_items,
    highlight_state,
    app.selected_playlist_index,
  );
}

pub fn draw_user_block(f: &mut Frame<'_>, app: &App, layout_chunk: Rect) {
  // Check for width to make a responsive layout
  if app.size.width >= SMALL_TERMINAL_WIDTH && !app.user_config.behavior.enforce_wide_search_bar {
    let [input_area, library_area, playlist_area] = layout_chunk.layout(&Layout::vertical([
      Constraint::Length(3),
      Constraint::Percentage(30),
      Constraint::Percentage(70),
    ]));

    // Search input and help
    draw_input_and_help_box(f, app, input_area);
    draw_library_block(f, app, library_area);
    draw_playlist_block(f, app, playlist_area);
  } else {
    let [library_area, playlist_area] = layout_chunk.layout(&Layout::vertical([
      Constraint::Percentage(30),
      Constraint::Percentage(70),
    ]));

    // Search input and help
    draw_library_block(f, app, library_area);
    draw_playlist_block(f, app, playlist_area);
  }
}

pub fn draw_search_results(f: &mut Frame<'_>, app: &App, layout_chunk: Rect) {
  let [song_artist_area, albums_playlist_area, podcasts_area] =
    layout_chunk.layout(&Layout::vertical([
      Constraint::Percentage(35),
      Constraint::Percentage(35),
      Constraint::Percentage(25),
    ]));

  {
    let [songs_area, artists_area] = song_artist_area.layout(&Layout::horizontal([
      Constraint::Percentage(50),
      Constraint::Percentage(50),
    ]));

    let currently_playing_id = app
      .current_playback_context
      .clone()
      .and_then(|context| {
        context.item.and_then(|item| match item {
          PlayableItem::Track(track) => track.id.map(|id| id.id().to_string()),
          PlayableItem::Episode(episode) => Some(episode.id.id().to_string()),
        })
      })
      .unwrap_or_default();

    let songs = match &app.search_results.tracks {
      Some(tracks) => tracks
        .items
        .iter()
        .map(|item| {
          let mut song_name = "".to_string();
          let id = item
            .clone()
            .id
            .map(|id| id.id().to_string())
            .unwrap_or_else(|| "".to_string());
          if currently_playing_id == id {
            song_name += "▶ "
          }
          if app.liked_song_ids_set.contains(&id) {
            song_name += &app.user_config.padded_liked_icon();
          }

          song_name += &item.name;
          song_name += &format!(" - {}", &create_artist_string(&item.artists));
          song_name
        })
        .collect(),
      None => vec![],
    };

    draw_selectable_list(
      f,
      app,
      songs_area,
      "Songs",
      &songs,
      get_search_results_highlight_state(app, SearchResultBlock::SongSearch),
      app.search_results.selected_tracks_index,
    );

    let artists = match &app.search_results.artists {
      Some(artists) => artists
        .items
        .iter()
        .map(|item| {
          let mut artist = String::new();
          if app.followed_artist_ids_set.contains(item.id.id()) {
            artist.push_str(&app.user_config.padded_liked_icon());
          }
          artist.push_str(&item.name.to_owned());
          artist
        })
        .collect(),
      None => vec![],
    };

    draw_selectable_list(
      f,
      app,
      artists_area,
      "Artists",
      &artists,
      get_search_results_highlight_state(app, SearchResultBlock::ArtistSearch),
      app.search_results.selected_artists_index,
    );
  }

  {
    let [albums_area, playlist_area] = albums_playlist_area.layout(&Layout::horizontal([
      Constraint::Percentage(50),
      Constraint::Percentage(50),
    ]));

    let albums = match &app.search_results.albums {
      Some(albums) => albums
        .items
        .iter()
        .map(|item| {
          let mut album_artist = String::new();
          if let Some(album_id) = &item.id {
            if app.saved_album_ids_set.contains(album_id.id()) {
              album_artist.push_str(&app.user_config.padded_liked_icon());
            }
          }
          album_artist.push_str(&format!(
            "{} - {} ({})",
            item.name.to_owned(),
            create_artist_string(&item.artists),
            item.album_type.as_deref().unwrap_or("unknown")
          ));
          album_artist
        })
        .collect(),
      None => vec![],
    };

    draw_selectable_list(
      f,
      app,
      albums_area,
      "Albums",
      &albums,
      get_search_results_highlight_state(app, SearchResultBlock::AlbumSearch),
      app.search_results.selected_album_index,
    );

    let playlists = match &app.search_results.playlists {
      Some(playlists) => playlists
        .items
        .iter()
        .map(|item| item.name.to_owned())
        .collect::<Vec<String>>(),
      None => vec![],
    };

    if playlists.is_empty() {
      let warning_text = "Cannot display Spotify created playlists. Try a more specific search to find user-created playlists.";
      let warning_paragraph = Paragraph::new(warning_text)
        .wrap(Wrap { trim: true })
        .style(Style::default().fg(app.user_config.theme.hint))
        .block(
          Block::default()
            .title(Span::styled(
              "Playlists",
              get_color(
                get_search_results_highlight_state(app, SearchResultBlock::PlaylistSearch),
                app.user_config.theme,
              ),
            ))
            .borders(Borders::ALL)
            .border_style(get_color(
              get_search_results_highlight_state(app, SearchResultBlock::PlaylistSearch),
              app.user_config.theme,
            )),
        );
      f.render_widget(warning_paragraph, playlist_area);
    } else {
      draw_selectable_list(
        f,
        app,
        playlist_area,
        "Playlists",
        &playlists,
        get_search_results_highlight_state(app, SearchResultBlock::PlaylistSearch),
        app.search_results.selected_playlists_index,
      );
    }
  }

  {
    draw_selectable_list(
      f,
      app,
      podcasts_area,
      "Podcasts",
      &match &app.search_results.shows {
        Some(podcasts) => podcasts
          .items
          .iter()
          .map(|item| {
            let mut show_name = String::new();
            if app.saved_show_ids_set.contains(item.id.id()) {
              show_name.push_str(&app.user_config.padded_liked_icon());
            }
            show_name.push_str(&format!("{:} - {}", item.name, item.publisher));
            show_name
          })
          .collect(),
        None => vec![],
      },
      get_search_results_highlight_state(app, SearchResultBlock::ShowSearch),
      app.search_results.selected_shows_index,
    );
  }
}

struct AlbumUi {
  selected_index: usize,
  items: Vec<TableItem>,
  title: String,
}

pub fn draw_artist_table(f: &mut Frame<'_>, app: &App, layout_chunk: Rect) {
  let header = TableHeader {
    id: TableId::Artist,
    items: vec![TableHeaderItem {
      text: "Artist",
      width: get_percentage_width(layout_chunk.width, 1.0),
      ..Default::default()
    }],
  };

  let current_route = app.get_current_route();
  let highlight_state = (
    current_route.active_block == ActiveBlock::Artists,
    current_route.hovered_block == ActiveBlock::Artists,
  );
  let items = app
    .artists
    .iter()
    .map(|item| TableItem {
      id: item.id.id().to_string(),
      format: vec![item.name.to_owned()],
    })
    .collect::<Vec<TableItem>>();

  draw_table(
    f,
    app,
    layout_chunk,
    ("Artists", &header),
    &items,
    app.artists_list_index,
    highlight_state,
  )
}

pub fn draw_podcast_table(f: &mut Frame<'_>, app: &App, layout_chunk: Rect) {
  let header = TableHeader {
    id: TableId::Podcast,
    items: vec![
      TableHeaderItem {
        text: "Name",
        width: get_percentage_width(layout_chunk.width, 2.0 / 5.0),
        ..Default::default()
      },
      TableHeaderItem {
        text: "Publisher(s)",
        width: get_percentage_width(layout_chunk.width, 2.0 / 5.0),
        ..Default::default()
      },
    ],
  };

  let current_route = app.get_current_route();

  let highlight_state = (
    current_route.active_block == ActiveBlock::Podcasts,
    current_route.hovered_block == ActiveBlock::Podcasts,
  );

  if let Some(saved_shows) = app.library.saved_shows.get_results(None) {
    let items = saved_shows
      .items
      .iter()
      .map(|show_page| TableItem {
        id: show_page.show.id.id().to_string(),
        format: vec![
          show_page.show.name.to_owned(),
          show_page.show.publisher.to_owned(),
        ],
      })
      .collect::<Vec<TableItem>>();

    draw_table(
      f,
      app,
      layout_chunk,
      ("Podcasts", &header),
      &items,
      app.shows_list_index,
      highlight_state,
    )
  };
}

pub fn draw_album_table(f: &mut Frame<'_>, app: &App, layout_chunk: Rect) {
  let header = TableHeader {
    id: TableId::Album,
    items: vec![
      TableHeaderItem {
        id: ColumnId::Liked,
        text: "",
        width: 2,
      },
      TableHeaderItem {
        text: "#",
        width: 3,
        ..Default::default()
      },
      TableHeaderItem {
        id: ColumnId::Title,
        text: "Title",
        width: get_percentage_width(layout_chunk.width, 2.0 / 5.0) - 5,
      },
      TableHeaderItem {
        text: "Artist",
        width: get_percentage_width(layout_chunk.width, 2.0 / 5.0),
        ..Default::default()
      },
      TableHeaderItem {
        text: "Length",
        width: get_percentage_width(layout_chunk.width, 1.0 / 5.0),
        ..Default::default()
      },
    ],
  };

  let current_route = app.get_current_route();
  let highlight_state = (
    current_route.active_block == ActiveBlock::AlbumTracks,
    current_route.hovered_block == ActiveBlock::AlbumTracks,
  );

  let album_ui = match &app.album_table_context {
    AlbumTableContext::Simplified => {
      app
        .selected_album_simplified
        .as_ref()
        .map(|selected_album_simplified| AlbumUi {
          items: selected_album_simplified
            .tracks
            .items
            .iter()
            .map(|item| TableItem {
              id: item
                .id
                .as_ref()
                .map(|id| id.id().to_string())
                .unwrap_or_else(|| "".to_string()),
              format: vec![
                "".to_string(),
                item.track_number.to_string(),
                item.name.to_owned(),
                create_artist_string(&item.artists),
                millis_to_minutes(item.duration.num_milliseconds() as u128),
              ],
            })
            .collect::<Vec<TableItem>>(),
          title: format!(
            "{} by {}",
            selected_album_simplified.album.name,
            create_artist_string(&selected_album_simplified.album.artists)
          ),
          selected_index: selected_album_simplified.selected_index,
        })
    }
    AlbumTableContext::Full => match app.selected_album_full.clone() {
      Some(selected_album) => Some(AlbumUi {
        items: selected_album
          .album
          .tracks
          .items
          .iter()
          .map(|item| TableItem {
            id: item
              .id
              .as_ref()
              .map(|id| id.id().to_string())
              .unwrap_or_else(|| "".to_string()),
            format: vec![
              "".to_string(),
              item.track_number.to_string(),
              item.name.to_owned(),
              create_artist_string(&item.artists),
              millis_to_minutes(item.duration.num_milliseconds() as u128),
            ],
          })
          .collect::<Vec<TableItem>>(),
        title: format!(
          "{} by {}",
          selected_album.album.name,
          create_artist_string(&selected_album.album.artists)
        ),
        selected_index: app.saved_album_tracks_index,
      }),
      None => None,
    },
  };

  if let Some(album_ui) = album_ui {
    draw_table(
      f,
      app,
      layout_chunk,
      (&album_ui.title, &header),
      &album_ui.items,
      album_ui.selected_index,
      highlight_state,
    );
  };
}

pub fn draw_recommendations_table(f: &mut Frame<'_>, app: &App, layout_chunk: Rect) {
  let header = TableHeader {
    id: TableId::Song,
    items: vec![
      TableHeaderItem {
        id: ColumnId::Liked,
        text: "",
        width: 2,
      },
      TableHeaderItem {
        id: ColumnId::Title,
        text: "Title",
        width: get_percentage_width(layout_chunk.width, 0.3),
      },
      TableHeaderItem {
        text: "Artist",
        width: get_percentage_width(layout_chunk.width, 0.3),
        ..Default::default()
      },
      TableHeaderItem {
        text: "Album",
        width: get_percentage_width(layout_chunk.width, 0.3),
        ..Default::default()
      },
      TableHeaderItem {
        text: "Length",
        width: get_percentage_width(layout_chunk.width, 0.1),
        ..Default::default()
      },
    ],
  };

  let current_route = app.get_current_route();
  let highlight_state = (
    current_route.active_block == ActiveBlock::TrackTable,
    current_route.hovered_block == ActiveBlock::TrackTable,
  );

  let items = app
    .track_table
    .tracks
    .iter()
    .map(|item| TableItem {
      id: item
        .id
        .as_ref()
        .map(|id| id.id().to_string())
        .unwrap_or_else(|| "".to_string()),
      format: vec![
        "".to_string(),
        item.name.to_owned(),
        create_artist_string(&item.artists),
        item.album.name.to_owned(),
        millis_to_minutes(item.duration.num_milliseconds() as u128),
      ],
    })
    .collect::<Vec<TableItem>>();
  // match RecommendedContext
  let recommendations_ui = match &app.recommendations_context {
    Some(RecommendationsContext::Song) => format!(
      "Recommendations based on Song \'{}\'",
      &app.recommendations_seed
    ),
    Some(RecommendationsContext::Artist) => format!(
      "Recommendations based on Artist \'{}\'",
      &app.recommendations_seed
    ),
    None => "Recommendations".to_string(),
  };
  draw_table(
    f,
    app,
    layout_chunk,
    (&recommendations_ui[..], &header),
    &items,
    app.track_table.selected_index,
    highlight_state,
  )
}

pub fn draw_song_table(f: &mut Frame<'_>, app: &App, layout_chunk: Rect) {
  let header = TableHeader {
    id: TableId::Song,
    items: vec![
      TableHeaderItem {
        id: ColumnId::Liked,
        text: "",
        width: 2,
      },
      TableHeaderItem {
        id: ColumnId::Title,
        text: "Title",
        width: get_percentage_width(layout_chunk.width, 0.3),
      },
      TableHeaderItem {
        text: "Artist",
        width: get_percentage_width(layout_chunk.width, 0.3),
        ..Default::default()
      },
      TableHeaderItem {
        text: "Album",
        width: get_percentage_width(layout_chunk.width, 0.3),
        ..Default::default()
      },
      TableHeaderItem {
        text: "Length",
        width: get_percentage_width(layout_chunk.width, 0.1),
        ..Default::default()
      },
    ],
  };

  let current_route = app.get_current_route();
  let highlight_state = (
    current_route.active_block == ActiveBlock::TrackTable,
    current_route.hovered_block == ActiveBlock::TrackTable,
  );

  let items = app
    .track_table
    .tracks
    .iter()
    .map(|item| TableItem {
      id: item
        .id
        .as_ref()
        .map(|id| id.id().to_string())
        .unwrap_or_else(|| "".to_string()),
      format: vec![
        "".to_string(),
        item.name.to_owned(),
        create_artist_string(&item.artists),
        item.album.name.to_owned(),
        millis_to_minutes(item.duration.num_milliseconds() as u128),
      ],
    })
    .collect::<Vec<TableItem>>();

  draw_table(
    f,
    app,
    layout_chunk,
    ("Songs", &header),
    &items,
    app.track_table.selected_index,
    highlight_state,
  )
}

pub fn draw_basic_view(f: &mut Frame<'_>, app: &App) {
  let chunks = Layout::default()
    .direction(Direction::Vertical)
    .constraints(
      [
        Constraint::Min(0), // Lyrics Area taking all available space above
        Constraint::Length(BASIC_VIEW_HEIGHT), // Playbar at the bottom
      ]
      .as_ref(),
    )
    .split(f.area());

  draw_lyrics(f, app, chunks[0]);
  draw_playbar(f, app, chunks[1]);
}

fn draw_lyrics(f: &mut Frame<'_>, app: &App, area: Rect) {
  use crate::app::LyricsStatus;

  // Draw bordered block first
  let block = Block::default()
    .borders(Borders::ALL)
    .title(" Lyrics ")
    .style(Style::default().fg(Color::Rgb(100, 100, 100))); // RGB for cross-terminal compat
  f.render_widget(block.clone(), area);

  let inner_area = block.inner(area);

  if app.lyrics_status != LyricsStatus::Found {
    let text = match app.lyrics_status {
      LyricsStatus::Loading => "Loading lyrics...",
      LyricsStatus::NotFound => "No lyrics found for this track.",
      LyricsStatus::NotStarted => "Waiting for track update...",
      LyricsStatus::Found => "",
    };

    if !text.is_empty() {
      let p = Paragraph::new(text)
        .style(Style::default().fg(Color::Rgb(100, 100, 100))) // RGB for cross-terminal compat
        .alignment(Alignment::Center);

      // Center vertically in inner area
      let vertical_center = inner_area.y + inner_area.height / 2;
      let top_area = Rect {
        x: inner_area.x,
        y: vertical_center.saturating_sub(0), // Just one line centered
        width: inner_area.width,
        height: 1,
      };
      f.render_widget(p, top_area);
    }
    return;
  }

  if let Some(lyrics) = &app.lyrics {
    if lyrics.is_empty() {
      return;
    }

    let current_time = app.song_progress_ms;
    let mut active_idx = 0;
    for (i, (time, _)) in lyrics.iter().enumerate() {
      if *time <= current_time {
        active_idx = i;
      } else {
        break;
      }
    }

    // Target position for active line: Vertical center of inner_area
    let target_row = inner_area.y + (inner_area.height / 2);

    let area_height = inner_area.height as i32;
    let area_y = inner_area.y as i32;

    // Loop through all visible rows of the screen area
    for row in 0..area_height {
      let screen_y = area_y + row;

      // screen_y = target_row + (line_idx - active_idx)
      // line_idx = screen_y - target_row + active_idx

      let offset_from_target = screen_y - (target_row as i32);
      let line_idx = active_idx as i32 + offset_from_target;

      if line_idx >= 0 && line_idx < lyrics.len() as i32 {
        let (_, text) = &lyrics[line_idx as usize];
        let is_active = line_idx == active_idx as i32;

        // Use explicit RGB colors for cross-terminal compatibility
        // Some terminals (like Kitty with custom themes) remap ANSI colors
        let style = if is_active {
          Style::default()
            .fg(app.user_config.theme.highlighted_lyrics) // Use theme color for highlighted lyrics
            .add_modifier(Modifier::BOLD)
        } else {
          Style::default().fg(Color::Rgb(100, 100, 100)) // Dim gray for inactive lines
        };

        let p = Paragraph::new(text.clone())
          .style(style)
          .alignment(Alignment::Center);

        let line_rect = Rect {
          x: inner_area.x,
          y: screen_y as u16,
          width: inner_area.width,
          height: 1,
        };
        f.render_widget(p, line_rect);
      }
    }
  }
}

pub fn draw_playbar(f: &mut Frame<'_>, app: &App, layout_chunk: Rect) {
  let [artist_area, _, progress_area] = layout_chunk.layout(
    &Layout::vertical([
      Constraint::Percentage(50),
      Constraint::Percentage(25),
      Constraint::Percentage(25),
    ])
    .margin(1),
  );

  let mut drew_playbar = false;

  // If no track is playing, render paragraph showing which device is selected, if no selected
  // give hint to choose a device
  if let Some(current_playback_context) = &app.current_playback_context {
    if let Some(track_item) = &current_playback_context.item {
      // Use native playing state when streaming is active (more reliable for MPRIS controls)
      let is_playing = app
        .native_is_playing
        .filter(|_| app.is_streaming_active)
        .unwrap_or(current_playback_context.is_playing);

      let play_title = if is_playing { "Playing" } else { "Paused" };

      let shuffle_text = if current_playback_context.shuffle_state {
        "On"
      } else {
        "Off"
      };

      let repeat_text = match current_playback_context.repeat_state {
        RepeatState::Off => "Off",
        RepeatState::Track => "Track",
        RepeatState::Context => "All",
      };

      let mut title = format!(
        "{:-7} ({} | Shuffle: {:-3} | Repeat: {:-5} | Volume: {:-2}%)",
        play_title,
        current_playback_context.device.name,
        shuffle_text,
        repeat_text,
        current_playback_context.device.volume_percent.unwrap_or(0)
      );

      if let Some(message) = app.status_message.as_ref() {
        title = format!("{} | {}", title, message);
      }

      let current_route = app.get_current_route();
      let highlight_state = (
        current_route.active_block == ActiveBlock::PlayBar,
        current_route.hovered_block == ActiveBlock::PlayBar,
      );

      let title_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .style(Style::default().bg(app.user_config.theme.playbar_background))
        .title(Span::styled(
          &title,
          get_color(highlight_state, app.user_config.theme),
        ))
        .border_style(get_color(highlight_state, app.user_config.theme));

      f.render_widget(title_block, layout_chunk);

      let (item_id, name, duration) = match track_item {
        PlayableItem::Track(track) => (
          track
            .id
            .as_ref()
            .map(|id| id.id().to_string())
            .unwrap_or_default(),
          track.name.to_owned(),
          track.duration,
        ),
        PlayableItem::Episode(episode) => (
          episode.id.id().to_string(),
          episode.name.to_owned(),
          episode.duration,
        ),
      };

      // Use native track info for instant display when available (e.g., after skipping tracks)
      // Falls back to API data when native info is not available
      let (display_name, display_artists, display_duration_ms) =
        if let Some(ref native_info) = app.native_track_info {
          (
            native_info.name.clone(),
            native_info.artists_display.clone(),
            native_info.duration_ms as u64,
          )
        } else {
          let artists_str = match track_item {
            PlayableItem::Track(track) => create_artist_string(&track.artists),
            PlayableItem::Episode(episode) => format!("{} - {}", episode.name, episode.show.name),
          };
          (
            name.clone(),
            artists_str,
            duration.num_milliseconds() as u64,
          )
        };

      let track_name = if app.liked_song_ids_set.contains(&item_id) {
        format!("{}{}", &app.user_config.padded_liked_icon(), display_name)
      } else {
        display_name
      };

      let lines = Text::from(Span::styled(
        display_artists,
        Style::default().fg(app.user_config.theme.playbar_text),
      ));

      let artist = Paragraph::new(lines)
        .style(Style::default().fg(app.user_config.theme.playbar_text))
        .block(
          Block::default().title(Span::styled(
            &track_name,
            Style::default()
              .fg(app.user_config.theme.selected)
              .add_modifier(Modifier::BOLD),
          )),
        );
      f.render_widget(artist, artist_area);

      let progress_ms = match app.seek_ms {
        Some(seek_ms) => seek_ms,
        None => app.song_progress_ms,
      };

      let duration_std = std::time::Duration::from_millis(display_duration_ms);
      let perc = get_track_progress_percentage(progress_ms, duration_std);

      let song_progress_label = display_track_progress(progress_ms, duration_std);
      let modifier = if app.user_config.behavior.enable_text_emphasis {
        Modifier::ITALIC | Modifier::BOLD
      } else {
        Modifier::empty()
      };
      let song_progress = LineGauge::default()
        .filled_style(
          Style::default()
            .fg(app.user_config.theme.playbar_progress)
            .add_modifier(modifier),
        )
        .unfilled_style(
          Style::default()
            .fg(app.user_config.theme.playbar_background)
            .add_modifier(modifier),
        )
        .ratio(perc as f64 / 100.0)
        .filled_symbol("⣿")
        .unfilled_symbol("⣉")
        .label(Span::styled(
          &song_progress_label,
          Style::default().fg(app.user_config.theme.playbar_progress_text),
        ));
      f.render_widget(song_progress, progress_area);

      // Draw "Like" animation (heart burst) if active
      if let Some(frame) = app.liked_song_animation_frame {
        let progress = (10 - frame) as f64;
        let y_base = 20.0 + progress * 5.0; // Rise up

        let canvas = Canvas::default()
          .block(Block::default()) // No border, transparent
          .x_bounds([0.0, 100.0])
          .y_bounds([0.0, 100.0])
          .paint(|ctx| {
            let color = app.user_config.theme.selected;
            // Center heart
            ctx.print(50.0, y_base, Span::styled("♥", Style::default().fg(color)));
            // Left particle (lagging slightly)
            ctx.print(
              48.0,
              y_base - 3.0,
              Span::styled("♥", Style::default().fg(color)),
            );
            // Right particle (lagging slightly)
            ctx.print(
              52.0,
              y_base - 3.0,
              Span::styled("♥", Style::default().fg(color)),
            );
          });

        f.render_widget(canvas, layout_chunk);
      }

      drew_playbar = true;
    }
  }

  if !drew_playbar {
    if let Some(message) = app.status_message.as_ref() {
      let title_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .style(Style::default().bg(app.user_config.theme.playbar_background))
        .title(Span::styled(
          format!("Status: {}", message),
          Style::default().fg(app.user_config.theme.playbar_text),
        ))
        .border_style(Style::default().fg(app.user_config.theme.inactive));
      f.render_widget(title_block, layout_chunk);
    }
  }
}

pub fn draw_error_screen(f: &mut Frame<'_>, app: &App) {
  let chunks = Layout::default()
    .direction(Direction::Vertical)
    .constraints([Constraint::Percentage(100)].as_ref())
    .margin(5)
    .split(f.area());

  let playing_text = vec![
    Line::from(vec![
      Span::raw("Api response: "),
      Span::styled(
        &app.api_error,
        Style::default().fg(app.user_config.theme.error_text),
      ),
    ]),
    Line::from(Span::styled(
      "If you are trying to play a track, please check that",
      Style::default().fg(app.user_config.theme.text),
    )),
    Line::from(Span::styled(
      " 1. You have a Spotify Premium Account",
      Style::default().fg(app.user_config.theme.text),
    )),
    Line::from(Span::styled(
      " 2. Your playback device is active and selected - press `d` to go to device selection menu",
      Style::default().fg(app.user_config.theme.text),
    )),
    Line::from(Span::styled(
      " 3. If you're using spotifyd as a playback device, your device name must not contain spaces",
      Style::default().fg(app.user_config.theme.text),
    )),
    Line::from(Span::styled("Hint: a playback device must be either an official spotify client or a light weight alternative such as spotifyd",
        Style::default().fg(app.user_config.theme.hint)
        ),
    ),
    Line::from(
      Span::styled(
          "\nPress <Esc> to return",
          Style::default().fg(app.user_config.theme.inactive),
      ),
    )
  ];

  let playing_paragraph = Paragraph::new(playing_text)
    .wrap(Wrap { trim: true })
    .style(app.user_config.theme.base_style())
    .block(
      Block::default()
        .borders(Borders::ALL)
        .style(app.user_config.theme.base_style())
        .title(Span::styled(
          "Error",
          Style::default().fg(app.user_config.theme.error_border),
        ))
        .border_style(Style::default().fg(app.user_config.theme.error_border)),
    );
  f.render_widget(playing_paragraph, chunks[0]);
}

fn draw_home(f: &mut Frame<'_>, app: &App, layout_chunk: Rect) {
  let [banner_area, changelog_area] = layout_chunk
    .layout(&Layout::vertical([Constraint::Length(7), Constraint::Length(93)]).margin(2));

  let current_route = app.get_current_route();
  let highlight_state = (
    current_route.active_block == ActiveBlock::Home,
    current_route.hovered_block == ActiveBlock::Home,
  );

  let welcome = Block::default()
    .title(Span::styled(
      "Welcome!",
      get_color(highlight_state, app.user_config.theme),
    ))
    .style(app.user_config.theme.base_style())
    .borders(Borders::ALL)
    .border_type(BorderType::Rounded)
    .border_style(get_color(highlight_state, app.user_config.theme));
  f.render_widget(welcome, layout_chunk);

  let (gradient_lines, base_changelog_lines) =
    get_home_cache(&app.user_config.theme, changelog_area.width);

  // Contains the banner
  let top_text = Paragraph::new(Text::from(gradient_lines))
    .style(app.user_config.theme.base_style())
    .block(Block::default());
  f.render_widget(top_text, banner_area);

  // Prepend global counter status to the changelog view
  let mut changelog_lines = Vec::with_capacity(base_changelog_lines.len() + 2);
  let counter_message = if cfg!(feature = "telemetry") {
    if app.user_config.behavior.enable_global_song_count {
      match app.global_song_count {
        Some(count) => format!("Global songs played with spotatui: {}", count),
        None if app.global_song_count_failed => {
          "Global song counter unavailable right now.".to_string()
        }
        None => "Loading global song count...".to_string(),
      }
    } else {
      "Global song counter disabled (Settings -> Behavior).".to_string()
    }
  } else {
    "Global song counter unavailable (telemetry disabled in this build).".to_string()
  };

  let counter_style = Style::default().fg(app.user_config.theme.hint);
  changelog_lines.push(Line::from(vec![Span::styled(
    counter_message,
    counter_style,
  )]));
  changelog_lines.push(Line::from(""));
  changelog_lines.extend(base_changelog_lines);

  // CHANGELOG
  let bottom_text = Paragraph::new(Text::from(changelog_lines))
    .block(Block::default())
    .style(app.user_config.theme.base_style())
    .wrap(Wrap { trim: false })
    .scroll((app.home_scroll, 0));
  f.render_widget(bottom_text, changelog_area);
}

fn get_clean_changelog() -> &'static str {
  CLEAN_CHANGELOG
    .get_or_init(|| {
      let changelog = include_str!("../../CHANGELOG.md");
      if cfg!(debug_assertions) {
        changelog.to_string()
      } else {
        changelog.replace("\n## [Unreleased]\n", "")
      }
    })
    .as_str()
}

fn get_home_cache(
  theme: &crate::user_config::Theme,
  changelog_width: u16,
) -> (Vec<Line<'static>>, Vec<Line<'static>>) {
  let cache = HOME_CACHE.get_or_init(|| Mutex::new(build_home_cache(theme, changelog_width)));
  let mut cache = cache.lock().expect("home cache lock failed");
  let theme_key = HomeThemeKey::from_theme(theme, changelog_width);
  if cache.theme_key != theme_key {
    *cache = build_home_cache(theme, changelog_width);
  }
  (cache.gradient_lines.clone(), cache.changelog_lines.clone())
}

fn build_home_cache(theme: &crate::user_config::Theme, changelog_width: u16) -> HomeCache {
  let changelog = get_clean_changelog();
  HomeCache {
    theme_key: HomeThemeKey::from_theme(theme, changelog_width),
    gradient_lines: build_banner_gradient_lines(theme),
    changelog_lines: build_changelog_lines(changelog, theme, changelog_width),
  }
}

fn build_banner_gradient_lines(theme: &crate::user_config::Theme) -> Vec<Line<'static>> {
  fn to_rgba(color: ratatui::style::Color) -> (u8, u8, u8, u8) {
    match color {
      ratatui::style::Color::Rgb(r, g, b) => (r, g, b, 255),
      ratatui::style::Color::Black => (0, 0, 0, 255),
      ratatui::style::Color::Red => (255, 0, 0, 255),
      ratatui::style::Color::Green => (0, 255, 0, 255),
      ratatui::style::Color::Yellow => (255, 255, 0, 255),
      ratatui::style::Color::Blue => (0, 0, 255, 255),
      ratatui::style::Color::Magenta => (255, 0, 255, 255),
      ratatui::style::Color::Cyan => (0, 255, 255, 255),
      ratatui::style::Color::Gray => (128, 128, 128, 255),
      ratatui::style::Color::DarkGray => (64, 64, 64, 255),
      ratatui::style::Color::LightRed => (255, 128, 128, 255),
      ratatui::style::Color::LightGreen => (128, 255, 128, 255),
      ratatui::style::Color::LightYellow => (255, 255, 128, 255),
      ratatui::style::Color::LightBlue => (128, 128, 255, 255),
      ratatui::style::Color::LightMagenta => (255, 128, 255, 255),
      ratatui::style::Color::LightCyan => (128, 255, 255, 255),
      ratatui::style::Color::White => (255, 255, 255, 255),
      _ => (255, 255, 255, 255),
    }
  }

  let c1 = to_rgba(theme.banner);
  let c2 = to_rgba(theme.active);
  let c3 = to_rgba(theme.hovered);

  let grad = colorgrad::GradientBuilder::new()
    .colors(&[
      colorgrad::Color::from_rgba8(c1.0, c1.1, c1.2, c1.3),
      colorgrad::Color::from_rgba8(c2.0, c2.1, c2.2, c2.3),
      colorgrad::Color::from_rgba8(c3.0, c3.1, c3.2, c3.3),
    ])
    .build::<colorgrad::LinearGradient>()
    .unwrap();

  BANNER
    .lines()
    .enumerate()
    .map(|(i, line)| {
      let t = (i as f64) / 8.0;
      let [r, g, b, _] = grad.at(t as f32).to_rgba8();
      Line::from(Span::styled(
        line,
        Style::default().fg(ratatui::style::Color::Rgb(r, g, b)),
      ))
    })
    .collect()
}

#[derive(Clone)]
struct StyledSegment {
  text: String,
  style: Style,
}

fn parse_markdown_inline(text: &str, base_style: Style) -> Vec<StyledSegment> {
  let mut segments: Vec<StyledSegment> = Vec::new();
  let mut buffer = String::new();
  let mut chars = text.chars().peekable();
  let mut is_bold = false;

  while let Some(ch) = chars.next() {
    if ch == '*' && chars.peek() == Some(&'*') {
      if !buffer.is_empty() {
        let style = if is_bold {
          base_style.add_modifier(Modifier::BOLD)
        } else {
          base_style
        };
        segments.push(StyledSegment {
          text: std::mem::take(&mut buffer),
          style,
        });
      }
      chars.next();
      is_bold = !is_bold;
    } else {
      buffer.push(ch);
    }
  }

  if !buffer.is_empty() {
    let style = if is_bold {
      base_style.add_modifier(Modifier::BOLD)
    } else {
      base_style
    };
    segments.push(StyledSegment {
      text: buffer,
      style,
    });
  }

  segments
}

fn segments_to_spans(segments: Vec<StyledSegment>) -> Vec<Span<'static>> {
  segments
    .into_iter()
    .map(|segment| Span::styled(segment.text, segment.style))
    .collect()
}

fn split_segments_by_whitespace(segments: &[StyledSegment]) -> Vec<StyledSegment> {
  let mut tokens = Vec::new();

  for segment in segments {
    let mut buffer = String::new();
    let mut buffer_is_whitespace: Option<bool> = None;

    for ch in segment.text.chars() {
      let is_whitespace = ch.is_whitespace();
      match buffer_is_whitespace {
        Some(current_state) if current_state == is_whitespace => buffer.push(ch),
        Some(_) => {
          tokens.push(StyledSegment {
            text: std::mem::take(&mut buffer),
            style: segment.style,
          });
          buffer.push(ch);
          buffer_is_whitespace = Some(is_whitespace);
        }
        None => {
          buffer.push(ch);
          buffer_is_whitespace = Some(is_whitespace);
        }
      }
    }

    if !buffer.is_empty() {
      tokens.push(StyledSegment {
        text: buffer,
        style: segment.style,
      });
    }
  }

  tokens
}

fn wrap_segments_with_indent(
  segments: &[StyledSegment],
  max_width: usize,
  prefix: &str,
  prefix_style: Style,
  indent: &str,
  indent_style: Style,
) -> Vec<Line<'static>> {
  let prefix_width = UnicodeWidthStr::width(prefix);
  let indent_width = UnicodeWidthStr::width(indent);
  let mut lines: Vec<Line<'static>> = Vec::new();
  let tokens = split_segments_by_whitespace(segments);
  let mut current: Vec<StyledSegment> = Vec::new();
  let mut current_width = 0;
  let mut is_first_line = true;

  for token in tokens {
    let token_width = UnicodeWidthStr::width(token.text.as_str());
    let is_whitespace = token.text.chars().all(char::is_whitespace);
    let available_width = if is_first_line {
      max_width.saturating_sub(prefix_width)
    } else {
      max_width.saturating_sub(indent_width)
    };

    if current_width == 0 && is_whitespace {
      continue;
    }

    if current_width + token_width > available_width && current_width > 0 {
      let prefix_to_use = if is_first_line { prefix } else { indent };
      let style_to_use = if is_first_line {
        prefix_style
      } else {
        indent_style
      };
      let mut spans = Vec::with_capacity(current.len() + 1);
      spans.push(Span::styled(prefix_to_use.to_string(), style_to_use));
      spans.extend(segments_to_spans(current));
      lines.push(Line::from(spans));

      current = Vec::new();
      current_width = 0;
      is_first_line = false;

      if is_whitespace {
        continue;
      }
    }

    current_width += token_width;
    current.push(token);
  }

  if !current.is_empty() || lines.is_empty() {
    let prefix_to_use = if is_first_line { prefix } else { indent };
    let style_to_use = if is_first_line {
      prefix_style
    } else {
      indent_style
    };
    let mut spans = Vec::with_capacity(current.len() + 1);
    spans.push(Span::styled(prefix_to_use.to_string(), style_to_use));
    spans.extend(segments_to_spans(current));
    lines.push(Line::from(spans));
  }

  lines
}

fn build_changelog_lines(
  changelog: &str,
  theme: &crate::user_config::Theme,
  max_width: u16,
) -> Vec<Line<'static>> {
  let mut lines: Vec<Line<'static>> = vec![];
  let max_width = usize::from(max_width);

  lines.push(Line::from(Span::styled(
    "Please report any bugs or missing features to https://github.com/LargeModGames/spotatui",
    Style::default().fg(theme.hint),
  )));
  lines.push(Line::from(""));

  for line in changelog.lines() {
    if line.starts_with("- ") {
      let content = line.trim_start_matches("- ");
      let segments = parse_markdown_inline(content, Style::default().fg(theme.text));
      let bullet_prefix = "  • ";
      let indent = " ".repeat(UnicodeWidthStr::width(bullet_prefix));
      lines.extend(wrap_segments_with_indent(
        &segments,
        max_width,
        bullet_prefix,
        Style::default().fg(theme.inactive),
        &indent,
        Style::default().fg(theme.text),
      ));
      continue;
    }

    let styled_line = if line.starts_with("# ") {
      Line::from(Span::styled(
        line.trim_start_matches("# ").to_string(),
        Style::default()
          .fg(theme.banner)
          .add_modifier(Modifier::BOLD),
      ))
    } else if line.starts_with("## [") {
      Line::from(Span::styled(
        format!("═══ {} ═══", line.trim_start_matches("## ")),
        Style::default()
          .fg(theme.active)
          .add_modifier(Modifier::BOLD),
      ))
    } else if line.starts_with("### ") {
      let section = line.trim_start_matches("### ");
      let color = match section {
        "Added" => theme.active,
        "Fixed" => theme.hint,
        "Changed" => theme.hovered,
        "Removed" | "Security" => theme.error_text,
        _ => theme.header,
      };
      Line::from(Span::styled(
        format!("  ┌─ {} ─┐", section),
        Style::default().fg(color).add_modifier(Modifier::BOLD),
      ))
    } else if line.is_empty() {
      Line::from("")
    } else {
      Line::from(segments_to_spans(parse_markdown_inline(
        line,
        Style::default().fg(theme.text),
      )))
    };
    lines.push(styled_line);
  }

  lines
}

fn draw_artist_albums(f: &mut Frame<'_>, app: &App, layout_chunk: Rect) {
  let [tracks_area, albums_area, related_artists_area] =
    layout_chunk.layout(&Layout::horizontal([
      Constraint::Percentage(33),
      Constraint::Percentage(33),
      Constraint::Percentage(33),
    ]));

  if let Some(artist) = &app.artist {
    let top_tracks = artist
      .top_tracks
      .iter()
      .map(|top_track| {
        let mut name = String::new();
        if let Some(context) = &app.current_playback_context {
          let track_id = match &context.item {
            Some(PlayableItem::Track(track)) => track.id.as_ref().map(|id| id.id().to_string()),
            Some(PlayableItem::Episode(episode)) => Some(episode.id.id().to_string()),
            _ => None,
          };

          if track_id == top_track.id.as_ref().map(|id| id.id().to_string()) {
            name.push_str("▶ ");
          }
        };
        name.push_str(&top_track.name);
        name
      })
      .collect::<Vec<String>>();

    draw_selectable_list(
      f,
      app,
      tracks_area,
      &format!("{} - Top Tracks", &artist.artist_name),
      &top_tracks,
      get_artist_highlight_state(app, ArtistBlock::TopTracks),
      Some(artist.selected_top_track_index),
    );

    let albums = &artist
      .albums
      .items
      .iter()
      .map(|item| {
        let mut album_artist = String::new();
        if let Some(album_id) = &item.id {
          if app.saved_album_ids_set.contains(album_id.id()) {
            album_artist.push_str(&app.user_config.padded_liked_icon());
          }
        }
        album_artist.push_str(&format!(
          "{} - {} ({})",
          item.name.to_owned(),
          create_artist_string(&item.artists),
          item.album_type.as_deref().unwrap_or("unknown")
        ));
        album_artist
      })
      .collect::<Vec<String>>();

    draw_selectable_list(
      f,
      app,
      albums_area,
      "Albums",
      albums,
      get_artist_highlight_state(app, ArtistBlock::Albums),
      Some(artist.selected_album_index),
    );

    let related_artists = artist
      .related_artists
      .iter()
      .map(|item| {
        let mut artist = String::new();
        if app.followed_artist_ids_set.contains(item.id.id()) {
          artist.push_str(&app.user_config.padded_liked_icon());
        }
        artist.push_str(&item.name.to_owned());
        artist
      })
      .collect::<Vec<String>>();

    draw_selectable_list(
      f,
      app,
      related_artists_area,
      "Related artists",
      &related_artists,
      get_artist_highlight_state(app, ArtistBlock::RelatedArtists),
      Some(artist.selected_related_artist_index),
    );
  };
}

pub fn draw_device_list(f: &mut Frame<'_>, app: &App) {
  let [instructions_area, list_area] = f
    .area()
    .layout(&Layout::vertical([Constraint::Percentage(20), Constraint::Percentage(80)]).margin(5));

  let device_instructions: Vec<Line> = vec![
        "To play tracks, please select a device. ",
        "Use `j/k` or up/down arrow keys to move up and down and <Enter> to select. ",
        "Your choice here will be cached so you can jump straight back in when you next open `spotatui`. ",
        "You can change the playback device at any time by pressing `d`.",
    ].into_iter().map(|instruction| Line::from(Span::raw(instruction))).collect();

  let instructions = Paragraph::new(device_instructions)
    .style(Style::default().fg(app.user_config.theme.text))
    .wrap(Wrap { trim: true })
    .block(
      Block::default().borders(Borders::NONE).title(Span::styled(
        "Welcome to spotatui!",
        Style::default()
          .fg(app.user_config.theme.active)
          .add_modifier(Modifier::BOLD),
      )),
    );
  f.render_widget(instructions, instructions_area);

  let no_device_message = Span::raw("No devices found: Make sure a device is active");

  let items = match &app.devices {
    Some(items) => {
      if items.devices.is_empty() {
        vec![ListItem::new(no_device_message)]
      } else {
        items
          .devices
          .iter()
          .map(|device| ListItem::new(Span::raw(&device.name)))
          .collect()
      }
    }
    None => vec![ListItem::new(no_device_message)],
  };

  let mut state = ListState::default();
  state.select(app.selected_device_index);
  let list = List::new(items)
    .block(
      Block::default()
        .title(Span::styled(
          "Devices",
          Style::default().fg(app.user_config.theme.active),
        ))
        .borders(Borders::ALL)
        .style(app.user_config.theme.base_style())
        .border_style(Style::default().fg(app.user_config.theme.inactive)),
    )
    .style(app.user_config.theme.base_style())
    .highlight_style(
      Style::default()
        .fg(app.user_config.theme.active)
        .bg(app.user_config.theme.inactive)
        .add_modifier(Modifier::BOLD),
    )
    .highlight_symbol(Line::from("▶ ").style(Style::default().fg(app.user_config.theme.active)));
  f.render_stateful_widget(list, list_area, &mut state);
}

pub fn draw_album_list(f: &mut Frame<'_>, app: &App, layout_chunk: Rect) {
  let header = TableHeader {
    id: TableId::AlbumList,
    items: vec![
      TableHeaderItem {
        text: "Name",
        width: get_percentage_width(layout_chunk.width, 2.0 / 5.0),
        ..Default::default()
      },
      TableHeaderItem {
        text: "Artists",
        width: get_percentage_width(layout_chunk.width, 2.0 / 5.0),
        ..Default::default()
      },
      TableHeaderItem {
        text: "Release Date",
        width: get_percentage_width(layout_chunk.width, 1.0 / 5.0),
        ..Default::default()
      },
    ],
  };

  let current_route = app.get_current_route();

  let highlight_state = (
    current_route.active_block == ActiveBlock::AlbumList,
    current_route.hovered_block == ActiveBlock::AlbumList,
  );

  let selected_song_index = app.album_list_index;

  if let Some(saved_albums) = app.library.saved_albums.get_results(None) {
    let items = saved_albums
      .items
      .iter()
      .map(|album_page| TableItem {
        id: album_page.album.id.id().to_string(),
        format: vec![
          format!(
            "{}{}",
            app.user_config.padded_liked_icon(),
            &album_page.album.name
          ),
          create_artist_string(&album_page.album.artists),
          album_page.album.release_date.to_owned(),
        ],
      })
      .collect::<Vec<TableItem>>();

    draw_table(
      f,
      app,
      layout_chunk,
      ("Saved Albums", &header),
      &items,
      selected_song_index,
      highlight_state,
    )
  };
}

pub fn draw_show_episodes(f: &mut Frame<'_>, app: &App, layout_chunk: Rect) {
  let header = TableHeader {
    id: TableId::PodcastEpisodes,
    items: vec![
      TableHeaderItem {
        // Column to mark an episode as fully played
        text: "",
        width: 2,
        ..Default::default()
      },
      TableHeaderItem {
        text: "Date",
        width: get_percentage_width(layout_chunk.width, 0.5 / 5.0) - 2,
        ..Default::default()
      },
      TableHeaderItem {
        text: "Name",
        width: get_percentage_width(layout_chunk.width, 3.5 / 5.0),
        id: ColumnId::Title,
      },
      TableHeaderItem {
        text: "Duration",
        width: get_percentage_width(layout_chunk.width, 1.0 / 5.0),
        ..Default::default()
      },
    ],
  };

  let current_route = app.get_current_route();

  let highlight_state = (
    current_route.active_block == ActiveBlock::EpisodeTable,
    current_route.hovered_block == ActiveBlock::EpisodeTable,
  );

  if let Some(episodes) = app.library.show_episodes.get_results(None) {
    let items = episodes
      .items
      .iter()
      .map(|episode| {
        let (played_str, time_str) = match episode.resume_point {
          Some(ResumePoint {
            fully_played,
            resume_position,
          }) => (
            if fully_played {
              " ✔".to_owned()
            } else {
              "".to_owned()
            },
            format!(
              "{} / {}",
              millis_to_minutes(resume_position.num_milliseconds() as u128),
              millis_to_minutes(episode.duration.num_milliseconds() as u128)
            ),
          ),
          None => (
            "".to_owned(),
            millis_to_minutes(episode.duration.num_milliseconds() as u128),
          ),
        };
        TableItem {
          id: episode.id.id().to_string(),
          format: vec![
            played_str,
            episode.release_date.to_owned(),
            episode.name.to_owned(),
            time_str,
          ],
        }
      })
      .collect::<Vec<TableItem>>();

    let title = match &app.episode_table_context {
      EpisodeTableContext::Simplified => match &app.selected_show_simplified {
        Some(selected_show) => {
          format!(
            "{} by {}",
            selected_show.show.name.to_owned(),
            selected_show.show.publisher
          )
        }
        None => "Episodes".to_owned(),
      },
      EpisodeTableContext::Full => match &app.selected_show_full {
        Some(selected_show) => {
          format!(
            "{} by {}",
            selected_show.show.name.to_owned(),
            selected_show.show.publisher
          )
        }
        None => "Episodes".to_owned(),
      },
    };

    draw_table(
      f,
      app,
      layout_chunk,
      (&title, &header),
      &items,
      app.episode_list_index,
      highlight_state,
    );
  };
}

pub fn draw_discover(f: &mut Frame<'_>, app: &App, layout_chunk: Rect) {
  let current_route = app.get_current_route();
  let highlight_state = (
    current_route.active_block == ActiveBlock::Discover,
    current_route.hovered_block == ActiveBlock::Discover,
  );

  // Split into two sections: playlist list and info panel
  let [list_area, info_area] = layout_chunk.layout(&Layout::vertical([
    Constraint::Min(8),
    Constraint::Length(8),
  ]));

  // Build discover options with status indicators
  let artists_mix_status = if app.discover_loading && app.discover_selected_index == 0 {
    " [Loading...]".to_string()
  } else if !app.discover_artists_mix.is_empty() {
    format!(" [{} tracks]", app.discover_artists_mix.len())
  } else {
    String::new()
  };

  let top_tracks_status = if app.discover_loading && app.discover_selected_index == 1 {
    " [Loading...]".to_string()
  } else if !app.discover_top_tracks.is_empty() {
    format!(" [{} tracks]", app.discover_top_tracks.len())
  } else {
    String::new()
  };

  let mut state = ListState::default();
  state.select(Some(app.discover_selected_index));

  let list_items: Vec<ListItem> = vec![
    // Top Artists Mix
    {
      let is_selected = app.discover_selected_index == 0;
      let prefix = if is_selected { "▸ " } else { "  " };
      let text_style = if is_selected {
        Style::default().fg(app.user_config.theme.selected)
      } else {
        Style::default().fg(app.user_config.theme.text)
      };
      ListItem::new(Line::from(vec![
        Span::styled(prefix, Style::default().fg(app.user_config.theme.selected)),
        Span::styled(
          format!("{} Top Artists Mix", app.user_config.padded_liked_icon()),
          text_style,
        ),
        Span::styled(
          artists_mix_status,
          Style::default().fg(app.user_config.theme.hint),
        ),
      ]))
    },
    // Top Tracks with time range
    {
      let is_selected = app.discover_selected_index == 1;
      let prefix = if is_selected { "▸ " } else { "  " };
      let text_style = if is_selected {
        Style::default().fg(app.user_config.theme.selected)
      } else {
        Style::default().fg(app.user_config.theme.text)
      };
      let time_range_label = format!(" ({})", app.discover_time_range.label());
      ListItem::new(Line::from(vec![
        Span::styled(prefix, Style::default().fg(app.user_config.theme.selected)),
        Span::styled(
          format!("{} Top Tracks", app.user_config.padded_liked_icon()),
          text_style,
        ),
        Span::styled(
          time_range_label,
          if is_selected {
            Style::default().fg(app.user_config.theme.active)
          } else {
            Style::default().fg(app.user_config.theme.inactive)
          },
        ),
        Span::styled(
          top_tracks_status,
          Style::default().fg(app.user_config.theme.hint),
        ),
      ]))
    },
  ];

  let list = List::new(list_items)
    .block(
      Block::default()
        .borders(Borders::ALL)
        .title(Span::styled(
          "Discover",
          get_color(highlight_state, app.user_config.theme),
        ))
        .border_style(get_color(highlight_state, app.user_config.theme)),
    )
    .highlight_style(get_color(highlight_state, app.user_config.theme).add_modifier(Modifier::BOLD))
    .highlight_symbol(Line::from("▶ ").style(get_color(highlight_state, app.user_config.theme)));

  f.render_stateful_widget(list, list_area, &mut state);

  // Info panel at bottom - context-sensitive help
  let info_lines = vec![
    Line::from(vec![
      Span::styled("↑/↓ ", Style::default().fg(app.user_config.theme.hint)),
      Span::styled("Navigate", Style::default().fg(app.user_config.theme.text)),
      Span::styled("   Enter ", Style::default().fg(app.user_config.theme.hint)),
      Span::styled("Select", Style::default().fg(app.user_config.theme.text)),
      if app.discover_selected_index == 1 {
        Span::styled("   [/] ", Style::default().fg(app.user_config.theme.hint))
      } else {
        Span::styled("", Style::default())
      },
      if app.discover_selected_index == 1 {
        Span::styled(
          "Time range",
          Style::default().fg(app.user_config.theme.text),
        )
      } else {
        Span::styled("", Style::default())
      },
    ]),
    Line::from(""),
    Line::from(vec![
      Span::styled(
        "Top Artists Mix: ",
        Style::default().fg(app.user_config.theme.text),
      ),
      Span::styled(
        "Shuffled tracks from your top 5 artists",
        Style::default().fg(app.user_config.theme.hint),
      ),
    ]),
    Line::from(vec![
      Span::styled(
        "Top Tracks: ",
        Style::default().fg(app.user_config.theme.text),
      ),
      Span::styled(
        "Your most-listened songs",
        Style::default().fg(app.user_config.theme.hint),
      ),
    ]),
    Line::from(""),
    Line::from(vec![
      Span::styled(
        format!("Time range: {} ", app.discover_time_range.label()),
        Style::default().fg(app.user_config.theme.text),
      ),
      Span::styled(
        "(4 weeks / 6 months / All time)",
        Style::default().fg(app.user_config.theme.inactive),
      ),
    ]),
  ];

  let info = Paragraph::new(info_lines)
    .block(
      Block::default()
        .borders(Borders::ALL)
        .title(Span::styled(
          "Help",
          Style::default().fg(app.user_config.theme.inactive),
        ))
        .border_style(Style::default().fg(app.user_config.theme.inactive)),
    )
    .wrap(Wrap { trim: true });

  f.render_widget(info, info_area);
}

pub fn draw_recently_played_table(f: &mut Frame<'_>, app: &App, layout_chunk: Rect) {
  let header = TableHeader {
    id: TableId::RecentlyPlayed,
    items: vec![
      TableHeaderItem {
        id: ColumnId::Liked,
        text: "",
        width: 2,
      },
      TableHeaderItem {
        id: ColumnId::Title,
        text: "Title",
        // We need to subtract the fixed value of the previous column
        width: get_percentage_width(layout_chunk.width, 2.0 / 5.0) - 2,
      },
      TableHeaderItem {
        text: "Artist",
        width: get_percentage_width(layout_chunk.width, 2.0 / 5.0),
        ..Default::default()
      },
      TableHeaderItem {
        text: "Length",
        width: get_percentage_width(layout_chunk.width, 1.0 / 5.0),
        ..Default::default()
      },
    ],
  };

  if let Some(recently_played) = &app.recently_played.result {
    let current_route = app.get_current_route();

    let highlight_state = (
      current_route.active_block == ActiveBlock::RecentlyPlayed,
      current_route.hovered_block == ActiveBlock::RecentlyPlayed,
    );

    let selected_song_index = app.recently_played.index;

    let items = recently_played
      .items
      .iter()
      .map(|item| TableItem {
        id: item
          .track
          .id
          .as_ref()
          .map(|id| id.id().to_string())
          .unwrap_or_else(|| "".to_string()),
        format: vec![
          "".to_string(),
          item.track.name.to_owned(),
          create_artist_string(&item.track.artists),
          millis_to_minutes(item.track.duration.num_milliseconds() as u128),
        ],
      })
      .collect::<Vec<TableItem>>();

    draw_table(
      f,
      app,
      layout_chunk,
      ("Recently Played Tracks", &header),
      &items,
      selected_song_index,
      highlight_state,
    )
  };
}

fn draw_selectable_list<S>(
  f: &mut Frame<'_>,
  app: &App,
  layout_chunk: Rect,
  title: &str,
  items: &[S],
  highlight_state: (bool, bool),
  selected_index: Option<usize>,
) where
  S: std::convert::AsRef<str>,
{
  let mut state = ListState::default();
  state.select(selected_index);

  let lst_items: Vec<ListItem> = items
    .iter()
    .map(|i| ListItem::new(Span::raw(i.as_ref())))
    .collect();

  let block = Block::default()
    .borders(Borders::ALL)
    .border_type(BorderType::Rounded)
    .title(Span::styled(
      title,
      get_color(highlight_state, app.user_config.theme),
    ))
    .border_style(get_color(highlight_state, app.user_config.theme));

  let list = List::new(lst_items)
    .block(block)
    .style(app.user_config.theme.base_style())
    .highlight_style(get_color(highlight_state, app.user_config.theme).add_modifier(Modifier::BOLD))
    .highlight_symbol(Line::from("▶ ").style(get_color(highlight_state, app.user_config.theme)));
  f.render_stateful_widget(list, layout_chunk, &mut state);
}

fn draw_dialog(f: &mut Frame<'_>, app: &App) {
  if let ActiveBlock::Dialog(_) = app.get_current_route().active_block {
    if let Some(playlist) = app.dialog.as_ref() {
      let bounds = f.area();
      // maybe do this better
      let width = std::cmp::min(bounds.width - 2, 45);
      let height = 8;
      let left = (bounds.width - width) / 2;
      let top = bounds.height / 4;

      let rect = Rect::new(left, top, width, height);

      f.render_widget(Clear, rect);

      let block = Block::default()
        .borders(Borders::ALL)
        .style(app.user_config.theme.base_style())
        .border_style(Style::default().fg(app.user_config.theme.inactive));

      f.render_widget(block, rect);

      let vchunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(2)
        .constraints([Constraint::Min(3), Constraint::Length(3)].as_ref())
        .split(rect);

      // suggestion: possibly put this as part of
      // app.dialog, but would have to introduce lifetime
      let text = vec![
        Line::from(Span::raw("Are you sure you want to delete the playlist: ")),
        Line::from(Span::styled(
          playlist.as_str(),
          Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::raw("?")),
      ];

      let text = Paragraph::new(text)
        .wrap(Wrap { trim: true })
        .style(app.user_config.theme.base_style())
        .alignment(Alignment::Center);

      f.render_widget(text, vchunks[0]);

      let hchunks = Layout::default()
        .direction(Direction::Horizontal)
        .horizontal_margin(3)
        .constraints([Constraint::Ratio(1, 2), Constraint::Ratio(1, 2)].as_ref())
        .split(vchunks[1]);

      let ok_text = Span::raw("Ok");
      let ok = Paragraph::new(ok_text)
        .style(Style::default().fg(if app.confirm {
          app.user_config.theme.hovered
        } else {
          app.user_config.theme.inactive
        }))
        .alignment(Alignment::Center);

      f.render_widget(ok, hchunks[0]);

      let cancel_text = Span::raw("Cancel");
      let cancel = Paragraph::new(cancel_text)
        .style(Style::default().fg(if app.confirm {
          app.user_config.theme.inactive
        } else {
          app.user_config.theme.hovered
        }))
        .alignment(Alignment::Center);

      f.render_widget(cancel, hchunks[1]);
    }
  }
}

fn draw_table(
  f: &mut Frame<'_>,
  app: &App,
  layout_chunk: Rect,
  table_layout: (&str, &TableHeader), // (title, header colums)
  items: &[TableItem], // The nested vector must have the same length as the `header_columns`
  selected_index: usize,
  highlight_state: (bool, bool),
) {
  let selected_style =
    get_color(highlight_state, app.user_config.theme).add_modifier(Modifier::BOLD);

  let track_playing_index = app.current_playback_context.to_owned().and_then(|ctx| {
    ctx.item.and_then(|item| match item {
      PlayableItem::Track(track) => {
        let track_id_str = track.id.map(|id| id.id().to_string());
        items.iter().position(|item| {
          track_id_str
            .as_ref()
            .map(|id| id == &item.id)
            .unwrap_or(false)
        })
      }
      PlayableItem::Episode(episode) => {
        let episode_id_str = episode.id.id().to_string();
        items.iter().position(|item| episode_id_str == item.id)
      }
    })
  });

  let (title, header) = table_layout;

  // Make sure that the selected item is visible on the page. Need to add some rows of padding
  // to chunk height for header and header space to get a true table height
  let padding = 5;
  let offset = layout_chunk
    .height
    .checked_sub(padding)
    .and_then(|height| selected_index.checked_sub(height as usize))
    .unwrap_or(0);

  let rows = items.iter().skip(offset).enumerate().map(|(i, item)| {
    let mut formatted_row = item.format.clone();
    let mut style = app.user_config.theme.base_style(); // default styling

    // if table displays songs
    match header.id {
      TableId::Song | TableId::RecentlyPlayed | TableId::Album => {
        // First check if the song should be highlighted because it is currently playing
        if let Some(title_idx) = header.get_index(ColumnId::Title) {
          if let Some(track_playing_offset_index) =
            track_playing_index.and_then(|idx| idx.checked_sub(offset))
          {
            if i == track_playing_offset_index {
              formatted_row[title_idx] = format!("▶ {}", &formatted_row[title_idx]);
              style = Style::default()
                .fg(app.user_config.theme.active)
                .add_modifier(Modifier::BOLD);
            }
          }
        }

        // Show this the liked icon if the song is liked
        if let Some(liked_idx) = header.get_index(ColumnId::Liked) {
          if app.liked_song_ids_set.contains(item.id.as_str()) {
            formatted_row[liked_idx] = app.user_config.padded_liked_icon();
          }
        }
      }
      TableId::PodcastEpisodes => {
        if let Some(name_idx) = header.get_index(ColumnId::Title) {
          if let Some(track_playing_offset_index) =
            track_playing_index.and_then(|idx| idx.checked_sub(offset))
          {
            if i == track_playing_offset_index {
              formatted_row[name_idx] = format!("▶ {}", &formatted_row[name_idx]);
              style = Style::default()
                .fg(app.user_config.theme.active)
                .add_modifier(Modifier::BOLD);
            }
          }
        }
      }
      _ => {}
    }

    // Next check if the item is under selection.
    if Some(i) == selected_index.checked_sub(offset) {
      style = selected_style;
    }

    // Return row styled data
    Row::new(formatted_row).style(style)
  });

  let widths = header
    .items
    .iter()
    .map(|h| Constraint::Length(h.width))
    .collect::<Vec<Constraint>>();

  let table = Table::new(rows, &widths)
    .header(
      Row::new(header.items.iter().map(|h| h.text))
        .style(Style::default().fg(app.user_config.theme.header)),
    )
    .block(
      Block::default()
        .borders(Borders::ALL)
        .style(app.user_config.theme.base_style())
        .title(Span::styled(
          title,
          get_color(highlight_state, app.user_config.theme),
        ))
        .border_style(get_color(highlight_state, app.user_config.theme)),
    )
    .style(app.user_config.theme.base_style());
  f.render_widget(table, layout_chunk);
}

/// Draw the mandatory update prompt modal
pub fn draw_update_prompt(f: &mut Frame<'_>, app: &App) {
  if let Some(update_info) = &app.update_available {
    let width = std::cmp::min(f.area().width.saturating_sub(4), 60);
    let height = 9;
    let rect = f
      .area()
      .centered(Constraint::Length(width), Constraint::Length(height));

    f.render_widget(Clear, rect);

    let text = vec![
      Line::from(Span::styled(
        "🚀 Update Available!",
        Style::default().add_modifier(Modifier::BOLD),
      )),
      Line::from(""),
      Line::from(format!(
        "Current: v{}  →  Latest: v{}",
        update_info.current_version, update_info.latest_version
      )),
      Line::from(""),
      Line::from("Run to update:"),
      Line::from(Span::styled(
        "  spotatui update --install",
        Style::default().add_modifier(Modifier::ITALIC),
      )),
      Line::from(""),
      Line::from(Span::styled(
        "[Press ENTER or ESC to continue]",
        Style::default().fg(app.user_config.theme.inactive),
      )),
    ];

    let paragraph = Paragraph::new(text)
      .style(app.user_config.theme.base_style())
      .alignment(Alignment::Center)
      .block(
        Block::default()
          .borders(Borders::ALL)
          .style(app.user_config.theme.base_style())
          .border_style(Style::default().fg(app.user_config.theme.active))
          .title(" Update Available "),
      );

    f.render_widget(paragraph, rect);
  }
}

/// Draw the sort menu popup overlay
fn draw_sort_menu(f: &mut Frame<'_>, app: &App) {
  if !app.sort_menu_visible {
    return;
  }

  let context = match app.sort_context {
    Some(ctx) => ctx,
    None => return,
  };

  let available_fields = context.available_fields();
  let current_sort = match context {
    crate::sort::SortContext::PlaylistTracks => &app.playlist_sort,
    crate::sort::SortContext::SavedAlbums => &app.album_sort,
    crate::sort::SortContext::SavedArtists => &app.artist_sort,
    crate::sort::SortContext::RecentlyPlayed => &app.playlist_sort,
  };

  let width = std::cmp::min(f.area().width.saturating_sub(4), 35);
  let height = (available_fields.len() + 4) as u16; // +4 for borders/padding
  let rect = f
    .area()
    .centered(Constraint::Length(width), Constraint::Length(height));

  f.render_widget(Clear, rect);

  // Build list items
  let items: Vec<ListItem> = available_fields
    .iter()
    .enumerate()
    .map(|(i, field)| {
      let shortcut = field
        .shortcut()
        .map(|c| format!(" ({})", c))
        .unwrap_or_default();
      let indicator = if *field == current_sort.field {
        format!(" {}", current_sort.order.indicator())
      } else {
        String::new()
      };
      let text = format!("{}{}{}", field.display_name(), shortcut, indicator);

      let style = if i == app.sort_menu_selected {
        Style::default()
          .fg(app.user_config.theme.active)
          .add_modifier(Modifier::BOLD)
      } else if *field == current_sort.field {
        Style::default().fg(app.user_config.theme.hovered)
      } else {
        Style::default().fg(app.user_config.theme.text)
      };

      ListItem::new(text).style(style)
    })
    .collect();

  let title = match context {
    crate::sort::SortContext::PlaylistTracks => "Sort Tracks",
    crate::sort::SortContext::SavedAlbums => "Sort Albums",
    crate::sort::SortContext::SavedArtists => "Sort Artists",
    crate::sort::SortContext::RecentlyPlayed => "Sort",
  };

  let list = List::new(items)
    .block(
      Block::default()
        .borders(Borders::ALL)
        .style(app.user_config.theme.base_style())
        .border_style(Style::default().fg(app.user_config.theme.active))
        .title(Span::styled(
          title,
          Style::default()
            .fg(app.user_config.theme.active)
            .add_modifier(Modifier::BOLD),
        )),
    )
    .highlight_style(
      Style::default()
        .fg(app.user_config.theme.active)
        .add_modifier(Modifier::BOLD),
    )
    .highlight_symbol(Line::from("▶ ").style(Style::default().fg(app.user_config.theme.active)));

  let mut state = ListState::default();
  state.select(Some(app.sort_menu_selected));

  f.render_stateful_widget(list, rect, &mut state);
}
