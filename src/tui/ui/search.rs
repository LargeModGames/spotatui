use crate::core::app::{ActiveBlock, App, InputContext, SearchResultBlock};
use crate::tui::layout::COMPACT_TOP_ROW_THRESHOLD;
use crate::tui::theme::ThemeExt;
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
/// areas (see `tui::layout::split_input_help_and_settings`). Splitting is
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

  let show_loading = (app.is_loading || app.is_source_track_table_loading())
    && app.user_config.behavior.show_loading_indicator;
  let border_type = if show_loading {
    BorderType::Double
  } else {
    BorderType::Rounded
  };

  let input_string: String = app.view.input.iter().collect();
  let lines = Text::from(input_string.clone());
  // Compute horizontal scroll so the cursor stays visible within the input box.
  // inner width = total width - 2 (for left and right borders)
  let inner_width = input_area.width.saturating_sub(2);
  let scroll_offset = if inner_width > 0 && app.view.input_cursor_position >= inner_width {
    app.view.input_cursor_position - inner_width + 1
  } else {
    0
  };
  app.view.input_scroll_offset.set(scroll_offset);

  let input_title = match app.view.input_context {
    InputContext::PlaylistTrackSearch => app.strings().screens.search_playlist,
    InputContext::SavedTrackSearch => app.strings().screens.search_liked,
    InputContext::GlobalSearch => app.strings().screens.search,
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
    (
      app.user_config.theme.inactive,
      app.strings().screens.type_help,
    )
  };

  let block = Block::default()
    .title(Span::styled(
      app.strings().screens.help,
      Style::default().fg(help_content.0.into()),
    ))
    .borders(Borders::ALL)
    .border_type(BorderType::Rounded)
    .border_style(Style::default().fg(help_content.0.into()));

  let lines = Text::from(help_content.1);
  let help = Paragraph::new(lines).block(block).style(
    Style::default()
      .fg(help_content.0.into())
      .bg(app.user_config.theme.background.into()),
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
    app.strings().screens.type_key(&settings_keybind_string)
  };
  let settings_color = app.user_config.theme.inactive;
  let settings_block = Block::default()
    .title(Span::styled(
      app.strings().screens.settings,
      Style::default().fg(settings_color.into()),
    ))
    .borders(Borders::ALL)
    .border_type(BorderType::Rounded)
    .border_style(Style::default().fg(settings_color.into()));

  let settings = Paragraph::new(settings_hint).block(settings_block).style(
    Style::default()
      .fg(settings_color.into())
      .bg(app.user_config.theme.background.into()),
  );
  f.render_widget(settings, settings_area);
}

/// Internet-radio search results: one full-area station list instead of the
/// five-block Spotify layout (a directory search returns only stations, so
/// Artists/Albums/Playlists/Podcasts would all be dead panels). Backed by the
/// same `SongSearch` block/index as the songs list so selection and Enter
/// share the existing machinery.
fn draw_radio_station_results(f: &mut Frame<'_>, app: &App, layout_chunk: Rect) {
  let stations: Vec<String> = match &app.search_results().tracks {
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
    app.strings().screens.radio_stations,
    &stations,
    get_search_results_highlight_state(app, SearchResultBlock::SongSearch),
    app.view.search_selected_tracks_index,
    &app.view.search_tracks_scroll_offset,
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

    let songs = match &app.search_results().tracks {
      Some(tracks) => tracks
        .items
        .iter()
        .map(|item| {
          let mut song_name = "".to_string();
          let id = item.id.clone().unwrap_or_default();
          if currently_playing_id == id {
            song_name += "▶ "
          }
          if app.liked_song_ids_set().contains(&id) {
            song_name += &app.user_config.padded_liked_icon();
          }

          song_name += item.display_name(app.strings());
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
      app.strings().screens.songs,
      &songs,
      get_search_results_highlight_state(app, SearchResultBlock::SongSearch),
      app.view.search_selected_tracks_index,
      &app.view.search_tracks_scroll_offset,
    );

    let artists = match &app.search_results().artists {
      Some(artists) => artists
        .items
        .iter()
        .map(|item| {
          let mut artist = String::new();
          if let Some(ref id) = item.id {
            if app.followed_artist_ids_set().contains(id.as_str()) {
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
      app.strings().screens.artists,
      &artists,
      get_search_results_highlight_state(app, SearchResultBlock::ArtistSearch),
      app.view.search_selected_artists_index,
      &app.view.search_artists_scroll_offset,
    );
  }

  {
    let [albums_area, playlist_area] = albums_playlist_area.layout(&Layout::horizontal([
      Constraint::Percentage(50),
      Constraint::Percentage(50),
    ]));

    let albums = match &app.search_results().albums {
      Some(albums) => albums
        .items
        .iter()
        .map(|item| {
          let mut album_artist = String::new();
          if let Some(ref id) = item.id {
            if app.saved_album_ids_set().contains(id.as_str()) {
              album_artist.push_str(&app.user_config.padded_liked_icon());
            }
          }
          album_artist.push_str(&format!(
            "{} - {} ({})",
            item.name,
            join_artist_names(&item.artists),
            app.strings().screens.album_type(item.album_type.as_deref())
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
      app.strings().screens.albums,
      &albums,
      get_search_results_highlight_state(app, SearchResultBlock::AlbumSearch),
      app.view.search_selected_album_index,
      &app.view.search_albums_scroll_offset,
    );

    let playlists = match &app.search_results().playlists {
      Some(playlists) => playlists
        .items
        .iter()
        .map(|item| item.name.to_owned())
        .collect::<Vec<String>>(),
      None => vec![],
    };

    if playlists.is_empty() {
      let warning_text = app.strings().screens.spotify_playlist_warning;
      let warning_paragraph = Paragraph::new(warning_text)
        .wrap(Wrap { trim: true })
        .style(Style::default().fg(app.user_config.theme.hint.into()))
        .block(
          Block::default()
            .title(Span::styled(
              app.strings().screens.playlists,
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
        app.strings().screens.playlists,
        &playlists,
        get_search_results_highlight_state(app, SearchResultBlock::PlaylistSearch),
        app.view.search_selected_playlists_index,
        &app.view.search_playlists_scroll_offset,
      );
    }
  }

  {
    draw_selectable_list(
      f,
      app,
      podcasts_area,
      app.strings().screens.podcasts,
      &match &app.search_results().shows {
        Some(podcasts) => podcasts
          .items
          .iter()
          .map(|item| {
            let mut show_name = String::new();
            if let Some(ref id) = item.id {
              if app.saved_show_ids_set().contains(id.as_str()) {
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
      app.view.search_selected_shows_index,
      &app.view.search_shows_scroll_offset,
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
      generated_name: None,
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
      release_date: None,
    }
  }

  /// Radio search results must render as one full-area Stations panel, not
  /// the five-block Spotify layout with stations shoved under "Songs".
  #[test]
  fn radio_results_render_single_station_panel() {
    let mut app = App::default();
    app.active_source = Source::Radio;
    app.set_search_results(crate::core::app::SearchResult {
      tracks: Some(Paged {
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
      }),
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

  #[test]
  fn search_album_types_translate_without_changing_album_titles() {
    use crate::core::i18n::Language;
    use crate::core::plugin_api::AlbumInfo;
    use unicode_width::UnicodeWidthStr;
    let mut app = App::default();
    app.set_search_results(crate::core::app::SearchResult {
      albums: Some(crate::core::pagination::Paged {
        items: vec![AlbumInfo {
          name: "single {title}".into(),
          album_type: Some("single".into()),
          ..Default::default()
        }],
        total: 1,
        ..Default::default()
      }),
      ..Default::default()
    });
    for language in [Language::En, Language::ZhHans, Language::ZhHant] {
      app.user_config.behavior.language = language;
      app.refresh_language();
      let mut terminal = Terminal::new(TestBackend::new(180, 40)).unwrap();
      terminal
        .draw(|frame| draw_search_results(frame, &app, frame.area()))
        .unwrap();
      let buffer = terminal.backend().buffer();
      let mut text = String::new();
      for y in 0..40 {
        let mut x = 0;
        while x < 180 {
          let symbol = buffer[(x, y)].symbol();
          text.push_str(symbol);
          x += symbol.width().max(1) as u16;
        }
      }
      assert!(text.contains("single {title}"), "{text}");
      assert!(
        text.contains(&format!("({})", app.strings().screens.album_type_single)),
        "{text}"
      );
    }
  }

  fn rendered_input(app: &App, area: Rect) -> String {
    let mut terminal = Terminal::new(TestBackend::new(area.width, area.height)).unwrap();
    terminal
      .draw(|f| {
        draw_input_and_help_box(
          f,
          app,
          area,
          // The help and settings boxes only matter for the compact-row
          // decision; the title under test is on the input box itself.
          Rect::new(0, 0, 0, 0),
          Rect::new(0, 0, 0, 0),
        )
      })
      .unwrap();
    let buffer = terminal.backend().buffer();
    (0..area.height)
      .flat_map(|y| (0..area.width).map(move |x| (x, y)))
      .filter_map(|(x, y)| buffer.cell((x, y)).map(|c| c.symbol().to_string()))
      .collect()
  }

  /// A decoded-source list keeps the global spinner off, so its own pending
  /// flag must still draw the search box's double-line loading border.
  #[test]
  fn a_pending_source_playlist_draws_the_loading_border() {
    let (tx, _rx) = std::sync::mpsc::channel();
    let mut app = App::new(tx, crate::core::user_config::UserConfig::new(), None);
    app.user_config.behavior.show_loading_indicator = true;
    let area = Rect::new(0, 0, 40, 3);
    assert!(!rendered_input(&app, area).contains('═'));

    app.open_source_playlist_tracks("subsonic:playlist:1".into());
    assert!(!app.is_loading);
    assert!(
      rendered_input(&app, area).contains('═'),
      "the open playlist should show the loading border"
    );
  }

  /// The input box title names the search it feeds, so a saved-track search is
  /// not mistaken for a global one.
  #[test]
  fn saved_track_search_input_titles_the_box() {
    let mut app = App::default();
    app.view.input_context = InputContext::SavedTrackSearch;
    app.view.input = "cherry".chars().collect();

    let content = rendered_input(&app, Rect::new(0, 0, 40, 3));

    assert!(
      content.contains("Search Liked Songs"),
      "the input box should title the saved-track search: {content}"
    );
    assert!(
      !content.contains("Search Playlist"),
      "the saved-track search is not the playlist one: {content}"
    );
  }
}
