use crate::core::app::{ActiveBlock, App, InputContext, SearchResultBlock};
use crate::core::layout::COMPACT_TOP_ROW_THRESHOLD;
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
  draw_selectable_list, get_color, get_search_results_highlight_state, join_artist_names,
};

/// Draws the search input, Help button and Settings button into pre-split
/// areas (see `core::layout::split_input_help_and_settings`). Splitting is
/// owned by `compute_main_layout` so mouse hit-testing always matches.
pub fn draw_input_and_help_box(
  f: &mut Frame<'_>,
  app: &App,
  input_area: Rect,
  help_area: Rect,
  settings_area: Rect,
) {
  let row_width = input_area
    .width
    .saturating_add(help_area.width)
    .saturating_add(settings_area.width);
  let compact_top_row = row_width < COMPACT_TOP_ROW_THRESHOLD;

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

  let input_title = match app.input_context {
    InputContext::PlaylistTrackSearch => "Search Playlist",
    InputContext::GlobalSearch => "Search",
  };

  let input = Paragraph::new(lines).scroll((0, scroll_offset)).block(
    Block::default()
      .borders(Borders::ALL)
      .border_type(border_type)
      .title(Span::styled(
        input_title,
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

/// Internet-radio search results: one full-area station list instead of the
/// five-block Spotify layout (a directory search returns only stations, so
/// Artists/Albums/Playlists/Podcasts would all be dead panels). Backed by the
/// same `SongSearch` block/index as the songs list so selection and Enter
/// share the existing machinery.
fn draw_radio_station_results(f: &mut Frame<'_>, app: &App, layout_chunk: Rect) {
  let stations: Vec<String> = match &app.search_results.tracks {
    Some(tracks) => tracks
      .items
      .iter()
      .map(|station| {
        let mut row = format!("\u{1F4FB} {}", station.name);
        // Genre tags land in `artists`, the country/codec/bitrate summary in
        // `album` (see infra::radio::station_to_track_info); both optional.
        if !station.artists.is_empty() {
          row += &format!(" - {}", station.artists.join(", "));
        }
        if !station.album.is_empty() {
          row += &format!("  ({})", station.album);
        }
        row
      })
      .collect(),
    None => vec![],
  };

  draw_selectable_list(
    f,
    app,
    layout_chunk,
    "Radio Stations",
    &stations,
    get_search_results_highlight_state(app, SearchResultBlock::SongSearch),
    app.search_results.selected_tracks_index,
  );
}

pub fn draw_search_results(f: &mut Frame<'_>, app: &App, layout_chunk: Rect) {
  if app.active_source == crate::core::source::Source::Radio {
    draw_radio_station_results(f, app, layout_chunk);
    return;
  }

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
          let id = item.id.clone().unwrap_or_default();
          if currently_playing_id == id {
            song_name += "▶ "
          }
          if app.liked_song_ids_set.contains(&id) {
            song_name += &app.user_config.padded_liked_icon();
          }

          song_name += &item.name;
          song_name += &format!(" - {}", item.artists.join(", "));
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
          if let Some(ref id) = item.id {
            if app.followed_artist_ids_set.contains(id.as_str()) {
              artist.push_str(&app.user_config.padded_liked_icon());
            }
          }
          artist.push_str(&item.name);
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
          if let Some(ref id) = item.id {
            if app.saved_album_ids_set.contains(id.as_str()) {
              album_artist.push_str(&app.user_config.padded_liked_icon());
            }
          }
          album_artist.push_str(&format!(
            "{} - {} ({})",
            item.name,
            join_artist_names(&item.artists),
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
            if let Some(ref id) = item.id {
              if app.saved_show_ids_set.contains(id.as_str()) {
                show_name.push_str(&app.user_config.padded_liked_icon());
              }
            }
            show_name.push_str(&item.name);
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

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::pagination::Paged;
  use crate::core::plugin_api::TrackInfo;
  use crate::core::source::Source;
  use ratatui::{backend::TestBackend, Terminal};

  fn rendered(app: &App, area: Rect) -> String {
    let mut terminal = Terminal::new(TestBackend::new(area.width, area.height)).unwrap();
    terminal
      .draw(|f| draw_search_results(f, app, area))
      .unwrap();
    let buffer = terminal.backend().buffer();
    (0..area.height)
      .flat_map(|y| (0..area.width).map(move |x| (x, y)))
      .filter_map(|(x, y)| buffer.cell((x, y)).map(|c| c.symbol().to_string()))
      .collect()
  }

  fn station(name: &str, tags: Vec<String>, summary: &str) -> TrackInfo {
    TrackInfo {
      uri: Some(format!("radio:https://example.com/{name}")),
      name: name.to_string(),
      artists: tags,
      album: summary.to_string(),
      duration_ms: 0,
      id: None,
      album_id: None,
      artist_refs: vec![],
      is_playable: true,
      is_local: false,
      track_number: 0,
      explicit: false,
      image_url: None,
    }
  }

  /// Radio search results must render as one full-area Stations panel, not
  /// the five-block Spotify layout with stations shoved under "Songs".
  #[test]
  fn radio_results_render_single_station_panel() {
    let mut app = App::default();
    app.active_source = Source::Radio;
    app.search_results.tracks = Some(Paged {
      items: vec![
        station(
          "Groove Salad",
          vec!["ambient".to_string(), "chillout".to_string()],
          "US \u{2022} MP3 \u{2022} 128 kbps",
        ),
        station("Bare FM", vec![], ""),
      ],
      total: 2,
      ..Default::default()
    });

    let content = rendered(&app, Rect::new(0, 0, 90, 30));

    assert!(
      content.contains("Radio Stations"),
      "panel title should be Radio Stations: {content}"
    );
    assert!(
      content.contains("Groove Salad - ambient, chillout"),
      "station rows should show name and tags: {content}"
    );
    for spotify_panel in ["Songs", "Artists", "Albums", "Playlists", "Podcasts"] {
      assert!(
        !content.contains(spotify_panel),
        "{spotify_panel} panel must not render under Radio: {content}"
      );
    }
    assert!(
      !content.contains("Bare FM -"),
      "a station without tags must not render a dangling separator: {content}"
    );
  }

  /// The Spotify layout is untouched for the default source.
  #[test]
  fn spotify_results_still_render_five_blocks() {
    let app = App::default();
    let content = rendered(&app, Rect::new(0, 0, 90, 30));
    for spotify_panel in ["Songs", "Artists", "Albums", "Playlists", "Podcasts"] {
      assert!(
        content.contains(spotify_panel),
        "{spotify_panel} panel should render under Spotify: {content}"
      );
    }
  }
}
