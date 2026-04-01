use crate::core::app::{ActiveBlock, App, SearchResultBlock};
use ratatui::{
  layout::{Constraint, Layout, Rect},
  style::Style,
  text::{Span, Text},
  widgets::{Block, BorderType, Borders, Paragraph, Wrap},
  Frame,
};

use rspotify::model::PlayableItem;
use rspotify::prelude::Id;

use super::util::{
  create_artist_string, draw_selectable_list, get_color, get_search_results_highlight_state,
  SMALL_TERMINAL_WIDTH,
};

const COMPACT_TOP_ROW_THRESHOLD: u16 = 60;
const COMPACT_HELP_WIDTH: u16 = 6;
const COMPACT_SETTINGS_WIDTH: u16 = 10;

pub fn draw_input_and_help_box(f: &mut Frame<'_>, app: &App, layout_chunk: Rect) {
  let compact_top_row = layout_chunk.width < COMPACT_TOP_ROW_THRESHOLD;

  // Check for the width and change the constraints accordingly
  let constraints = if compact_top_row {
    [
      Constraint::Min(1),
      Constraint::Length(COMPACT_HELP_WIDTH),
      Constraint::Length(COMPACT_SETTINGS_WIDTH),
    ]
  } else if app.size.width >= SMALL_TERMINAL_WIDTH
    && !app.user_config.behavior.enforce_wide_search_bar
  {
    [
      Constraint::Percentage(65),
      Constraint::Percentage(18),
      Constraint::Percentage(17),
    ]
  } else {
    [
      Constraint::Percentage(80),
      Constraint::Percentage(8),
      Constraint::Percentage(12),
    ]
  };

  let [input_area, help_area, settings_area] =
    layout_chunk.layout(&Layout::horizontal(constraints));

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

  let input = Paragraph::new(lines).scroll((0, scroll_offset)).block(
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

  let help_content = if show_loading {
    (app.user_config.theme.hint, "...")
  } else if compact_top_row {
    (app.user_config.theme.inactive, "?")
  } else {
    (app.user_config.theme.inactive, "Type ?")
  };

  let block = Block::default()
    .title(Span::styled("Help", Style::default().fg(help_content.0)))
    .borders(Borders::ALL)
    .border_type(BorderType::Rounded)
    .border_style(Style::default().fg(help_content.0));

  let lines = Text::from(help_content.1);
  let help = Paragraph::new(lines).block(block).style(
    Style::default()
      .fg(help_content.0)
      .bg(app.user_config.theme.background),
  );
  f.render_widget(help, help_area);

  let settings_keybind_string = app
    .effective_open_settings_key()
    .to_string()
    .trim_matches(|c| c == '<' || c == '>')
    .to_string();
  let settings_hint = if compact_top_row {
    settings_keybind_string
  } else {
    format!("Type {}", settings_keybind_string)
  };
  let settings_color = app.user_config.theme.inactive;
  let settings_block = Block::default()
    .title(Span::styled(
      "Settings",
      Style::default().fg(settings_color),
    ))
    .borders(Borders::ALL)
    .border_type(BorderType::Rounded)
    .border_style(Style::default().fg(settings_color));

  let settings = Paragraph::new(settings_hint).block(settings_block).style(
    Style::default()
      .fg(settings_color)
      .bg(app.user_config.theme.background),
  );
  f.render_widget(settings, settings_area);
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
          _ => None,
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
            #[allow(deprecated)]
            let publisher = &item.publisher;
            show_name.push_str(&format!("{:} - {}", item.name, publisher));
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
