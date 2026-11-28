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

pub(crate) fn search_result_panel_areas(area: Rect) -> [(SearchResultBlock, Rect); 5] {
  let [song_artist_area, albums_playlist_area, podcasts_area] = area.layout(&Layout::vertical([
    Constraint::Percentage(35),
    Constraint::Percentage(35),
    Constraint::Percentage(25),
  ]));
  let [songs_area, artists_area] = song_artist_area.layout(&Layout::horizontal([
    Constraint::Percentage(50),
    Constraint::Percentage(50),
  ]));
  let [albums_area, playlists_area] = albums_playlist_area.layout(&Layout::horizontal([
    Constraint::Percentage(50),
    Constraint::Percentage(50),
  ]));
  [
    (SearchResultBlock::SongSearch, songs_area),
    (SearchResultBlock::ArtistSearch, artists_area),
    (SearchResultBlock::AlbumSearch, albums_area),
    (SearchResultBlock::PlaylistSearch, playlists_area),
    (SearchResultBlock::ShowSearch, podcasts_area),
  ]
}

#[cfg(feature = "cover-art")]
pub(crate) fn browse_preview_height(app: &App, area: Rect) -> u16 {
  if !app.user_config.behavior.show_song_selection_details || area.width < 48 || area.height < 18 {
    return 0;
  }
  let height = area.height.saturating_sub(10).min(14);
  if height < 8 {
    0
  } else {
    height
  }
}

#[cfg(feature = "cover-art")]
const BROWSE_PREVIEW_ART_MARGIN: u16 = 1;

#[cfg(feature = "cover-art")]
const BROWSE_PREVIEW_ART_GAP_ROWS: u16 = 1;

#[cfg(feature = "cover-art")]
const BROWSE_PREVIEW_ART_GAP_COLS: u16 = BROWSE_PREVIEW_ART_GAP_ROWS * 2;

#[cfg(feature = "cover-art")]
fn browse_preview_info_area(info_area: Rect) -> Rect {
  let inset = BROWSE_PREVIEW_ART_GAP_ROWS;
  Rect {
    y: info_area.y.saturating_add(inset),
    height: info_area.height.saturating_sub(inset.saturating_mul(2)),
    ..info_area
  }
}

#[cfg(feature = "cover-art")]
fn browse_preview_art_area(inner: Rect, image_width: u16, gap_cols: u16) -> (Rect, Rect) {
  let margin = BROWSE_PREVIEW_ART_MARGIN;
  let reserved = margin.saturating_mul(2).saturating_add(1);
  let art_width = image_width
    .saturating_add(gap_cols.saturating_mul(2))
    .saturating_sub(margin.saturating_mul(2))
    .min(inner.width.saturating_sub(reserved) / 2)
    .min(28);
  if art_width >= 4 {
    let [_, art, _, info] = inner.layout(&Layout::horizontal([
      Constraint::Length(margin),
      Constraint::Length(art_width),
      Constraint::Length(margin),
      Constraint::Min(1),
    ]));
    (art, info)
  } else {
    (Rect::new(inner.x, inner.y, inner.width, 0), inner)
  }
}

#[cfg(feature = "cover-art")]
fn browse_preview_image_area(inner: Rect, info_area: Rect, fitted: Rect, gap_cols: u16) -> Rect {
  let region = Rect::new(
    inner.x,
    inner.y,
    info_area.x.saturating_sub(inner.x),
    inner.height,
  );
  let size = Rect::new(
    0,
    0,
    fitted
      .width
      .min(region.width.saturating_sub(gap_cols.saturating_mul(2))),
    fitted.height.min(
      region
        .height
        .saturating_sub(BROWSE_PREVIEW_ART_GAP_ROWS.saturating_mul(2)),
    ),
  );
  crate::tui::layout::center_rect_within(region, size)
}

#[cfg(feature = "cover-art")]
fn browse_preview_info_lines(
  track: &crate::core::plugin_api::TrackInfo,
  strings: &crate::core::i18n::Strings,
) -> Vec<ratatui::text::Line<'static>> {
  use ratatui::style::Modifier;
  use ratatui::text::{Line, Span};

  let field = |label: &'static str, value: String| {
    Line::from(vec![
      Span::styled(label, Style::default().add_modifier(Modifier::BOLD)),
      Span::raw(": "),
      Span::raw(value),
    ])
  };
  let mut lines = Vec::new();
  if !track.artists.is_empty() {
    lines.push(field(strings.screens.artist, track.artists.join(", ")));
  }
  if !track.album.is_empty() {
    lines.push(field(
      strings.screens.now_playing_album,
      track.album.clone(),
    ));
  }
  if let Some(year) = super::util::release_year(track.release_date.as_deref()) {
    lines.push(field(strings.screens.now_playing_year, year.to_string()));
  }
  if track.track_number > 0 {
    lines.push(field(
      strings.screens.now_playing_track,
      track.track_number.to_string(),
    ));
  }
  if track.duration_ms > 0 {
    lines.push(field(
      strings.screens.now_playing_length,
      crate::core::format::millis_to_minutes(u128::from(track.duration_ms)),
    ));
  }
  if track.explicit {
    lines.push(Line::from(Span::styled(
      strings.screens.now_playing_explicit,
      Style::default().add_modifier(Modifier::BOLD),
    )));
  }
  lines
}

#[cfg(feature = "cover-art")]
fn browse_preview_lines(
  item: &crate::core::app::BrowsePreviewItem,
  strings: &crate::core::i18n::Strings,
) -> Vec<ratatui::text::Line<'static>> {
  use crate::core::app::BrowsePreviewItem;
  use ratatui::style::Modifier;
  use ratatui::text::{Line, Span};

  let field = |label: &'static str, value: String| {
    Line::from(vec![
      Span::styled(label, Style::default().add_modifier(Modifier::BOLD)),
      Span::raw(": "),
      Span::raw(value),
    ])
  };
  match item {
    BrowsePreviewItem::Track(track) => browse_preview_info_lines(track, strings),
    BrowsePreviewItem::Playlist(playlist) => {
      let mut lines = Vec::new();
      if let Some(description) = playlist
        .description
        .as_deref()
        .filter(|text| !text.trim().is_empty())
      {
        lines.push(field(
          strings.screens.playlist_description,
          description.to_string(),
        ));
      }
      if !playlist.owner.is_empty() {
        lines.push(field(
          strings.screens.playlist_author,
          playlist.owner.clone(),
        ));
      }
      if playlist.track_count > 0 {
        lines.push(field(
          strings.screens.browse_preview_tracks,
          playlist.track_count.to_string(),
        ));
      }
      lines
    }
    BrowsePreviewItem::Artist(artist) => vec![field(strings.screens.artist, artist.name.clone())],
    BrowsePreviewItem::Album(album) => {
      let mut lines = Vec::new();
      if !album.artists.is_empty() {
        lines.push(field(
          strings.screens.artist,
          album
            .artists
            .iter()
            .map(|artist| artist.name.as_str())
            .collect::<Vec<_>>()
            .join(", "),
        ));
      }
      lines.push(field(
        strings.screens.browse_preview_type,
        strings
          .screens
          .album_type(album.album_type.as_deref())
          .to_string(),
      ));
      if let Some(year) = super::util::release_year(album.release_date.as_deref()) {
        lines.push(field(strings.screens.now_playing_year, year.to_string()));
      }
      if let Some(count) = album.total_tracks {
        lines.push(field(
          strings.screens.browse_preview_tracks,
          count.to_string(),
        ));
      }
      lines
    }
    BrowsePreviewItem::Show(show) => {
      let mut lines = Vec::new();
      if !show.publisher.is_empty() {
        lines.push(field(
          strings.screens.column_publisher,
          show.publisher.clone(),
        ));
      }
      if !show.description.trim().is_empty() {
        lines.push(field(
          strings.screens.playlist_description,
          show.description.clone(),
        ));
      }
      lines
    }
  }
}

#[cfg(feature = "cover-art")]
fn default_browse_preview_fitted(probe: Rect) -> Rect {
  let ratio = crate::tui::cover_art::cell_ratio().unwrap_or(2.0);
  let width = ((probe.height as f32) * ratio).round() as u16;
  let width = width.clamp(1, probe.width);
  let height = ((f32::from(width) / ratio).round() as u16).clamp(1, probe.height);
  Rect::new(0, 0, width, height)
}

#[cfg(feature = "cover-art")]
pub(crate) fn draw_browse_preview(f: &mut Frame<'_>, app: &App, area: Rect) {
  use ratatui::layout::Alignment;
  use ratatui::widgets::Clear;

  if area.width < 20 || area.height < 6 {
    return;
  }
  let theme = &app.user_config.theme;
  let preview_item = app.preview_item();
  let title = preview_item
    .as_ref()
    .map(|item| item.title(app.strings()))
    .filter(|name| !name.is_empty())
    .unwrap_or_else(|| app.strings().screens.song_cover.to_string());
  let block = Block::default()
    .borders(Borders::ALL)
    .border_type(BorderType::Rounded)
    .title(title)
    .border_style(Style::default().fg(theme.inactive.into()))
    .style(theme.base_style());
  let inner = block.inner(area);
  f.render_widget(Clear, area);
  f.render_widget(block, area);

  let lines = if let Some(item) = preview_item.as_ref() {
    browse_preview_lines(item, app.strings())
  } else {
    vec![ratatui::text::Line::from(
      super::util::browse_preview_cover_art_status(app),
    )]
  };
  let text_style = Style::default().fg(theme.text.into());
  if !app
    .user_config
    .do_draw_cover_art(crate::tui::cover_art::full_image_support())
  {
    f.render_widget(
      Paragraph::new(lines)
        .style(text_style)
        .wrap(Wrap { trim: true }),
      inner,
    );
    return;
  }
  let store = app.browse_preview_cover_art();
  let gap_cols = BROWSE_PREVIEW_ART_GAP_COLS;
  let image_height = inner
    .height
    .saturating_sub(BROWSE_PREVIEW_ART_GAP_ROWS.saturating_mul(2));
  let max_width = inner
    .width
    .saturating_sub(
      BROWSE_PREVIEW_ART_MARGIN
        .saturating_mul(2)
        .saturating_add(1),
    )
    .min(28);
  let probe = Rect::new(0, 0, max_width, image_height);
  let fitted = crate::tui::cover_art::browse_preview_size_for(probe, store)
    .unwrap_or_else(|| default_browse_preview_fitted(probe));
  let (art_area, info_area) = browse_preview_art_area(inner, fitted.width, gap_cols);
  if art_area.height > 0 && art_area.width > 0 {
    let image_area = browse_preview_image_area(inner, info_area, fitted, gap_cols);
    if store.available() {
      crate::tui::cover_art::render_browse_preview(f, image_area, store, &app.user_config);
    } else {
      let message = super::util::truncate_text(
        super::util::browse_preview_cover_art_status(app),
        usize::from(image_area.width),
      );
      f.render_widget(
        Paragraph::new(message)
          .style(Style::default().fg(theme.inactive.into()))
          .alignment(Alignment::Center),
        Rect {
          y: image_area.y + image_area.height / 2,
          height: 1,
          ..image_area
        },
      );
    }
  }
  f.render_widget(
    Paragraph::new(lines)
      .style(text_style)
      .wrap(Wrap { trim: true }),
    browse_preview_info_area(info_area),
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

  #[cfg(feature = "cover-art")]
  #[test]
  fn the_main_layout_shows_selected_song_info_below_the_song_list() {
    use crate::core::app::{ActiveBlock, RouteId, SearchResult, SearchResultBlock};
    use crate::core::geometry::Viewport;

    let mut app = App::default();
    app.view.size = Viewport {
      width: 90,
      height: 30,
    };
    app.push_navigation_stack(RouteId::Search, ActiveBlock::SearchResultBlock);
    app.set_search_results(SearchResult {
      tracks: Some(Paged {
        items: vec![TrackInfo {
          name: "Song".to_string(),
          artists: vec!["Artist".to_string()],
          album: "Album".to_string(),
          duration_ms: 195_000,
          release_date: Some("2024-06-28".to_string()),
          track_number: 3,
          explicit: true,
          image_url: Some("https://example.test/song.jpg".to_string()),
          ..station("Song", vec![], "")
        }],
        total: 1,
        ..Default::default()
      }),
      ..Default::default()
    });
    app.select_search_result_row(SearchResultBlock::SongSearch, Some(0));
    app.user_config.behavior.draw_cover_art_forced = true;
    let mut terminal = Terminal::new(TestBackend::new(90, 30)).unwrap();
    terminal
      .draw(|f| crate::tui::ui::draw_main_layout(f, &app))
      .unwrap();
    let buffer = terminal.backend().buffer();
    let content: String = (0..30)
      .flat_map(|y| (0..90).map(move |x| (x, y)))
      .filter_map(|(x, y)| buffer.cell((x, y)).map(|cell| cell.symbol().to_string()))
      .collect();
    let row = |y: u16| {
      (0..90)
        .map(|x| buffer[(x, y)].symbol().to_string())
        .collect::<String>()
    };
    let explicit_row = (0..30).find(|&y| row(y).contains("Explicit"));

    for expected in ["Artist", "Album", "2024", "3:15", "Explicit"] {
      assert!(
        content.contains(expected),
        "the selected-song info box should show {expected}: {content}"
      );
    }
    assert!(
      explicit_row.is_some_and(|y| y >= 14),
      "the info box should sit below the song list: {content}"
    );
  }

  #[cfg(feature = "cover-art")]
  #[test]
  fn hiding_song_selection_details_removes_the_bottom_box() {
    let mut app = App::default();
    app.user_config.behavior.draw_cover_art_forced = true;
    let area = Rect::new(0, 0, 90, 30);

    assert!(browse_preview_height(&app, area) > 0);
    app.user_config.behavior.show_song_selection_details = false;
    assert_eq!(browse_preview_height(&app, area), 0);
  }

  #[cfg(feature = "cover-art")]
  #[test]
  fn loading_cover_art_message_sits_in_the_middle_of_the_image_slot() {
    use crate::core::app::{ActiveBlock, RouteId, SearchResult, SearchResultBlock};
    use crate::core::art::CoverArtStatus;

    let mut app = App::default();
    app.push_navigation_stack(RouteId::Search, ActiveBlock::SearchResultBlock);
    app.set_search_results(SearchResult {
      tracks: Some(Paged {
        items: vec![station("Song", vec![], "")],
        total: 1,
        ..Default::default()
      }),
      ..Default::default()
    });
    app.select_search_result_row(SearchResultBlock::SongSearch, Some(0));
    app.browse_preview_cover_art_mut().status = CoverArtStatus::Loading;
    app.user_config.behavior.draw_cover_art_forced = true;

    let area = Rect::new(0, 0, 60, 12);
    let mut terminal = Terminal::new(TestBackend::new(area.width, area.height)).unwrap();
    terminal
      .draw(|f| draw_browse_preview(f, &app, area))
      .unwrap();
    let buffer = terminal.backend().buffer();
    let rows: Vec<u16> = (0..area.height)
      .filter(|&y| {
        (0..area.width)
          .map(|x| buffer[(x, y)].symbol().to_string())
          .collect::<String>()
          .contains("Loading")
      })
      .collect();

    let inner = Block::default()
      .borders(Borders::ALL)
      .border_type(BorderType::Rounded)
      .inner(area);
    let probe = Rect::new(
      0,
      0,
      inner.width.saturating_sub(3).min(28),
      inner.height.saturating_sub(2),
    );
    let (_, info) = browse_preview_art_area(inner, probe.width, BROWSE_PREVIEW_ART_GAP_COLS);
    let slot = browse_preview_image_area(inner, info, probe, BROWSE_PREVIEW_ART_GAP_COLS);

    assert_eq!(rows.len(), 1, "loading message should be one line");
    assert_eq!(rows[0], slot.y + slot.height / 2);
  }

  #[cfg(feature = "cover-art")]
  #[test]
  fn the_selected_playlist_shows_its_name_description_and_author() {
    use crate::core::app::{ActiveBlock, PlaylistFolderItem, RouteId};
    use crate::core::plugin_api::PlaylistInfo;

    let mut app = App::default();
    app.active_source = Source::Spotify;
    app.push_navigation_stack(RouteId::Search, ActiveBlock::MyPlaylists);
    *app.all_playlists_mut() = vec![PlaylistInfo {
      generated_name: None,
      uri: "spotify:playlist:road-trip".to_string(),
      name: "Road Trip".to_string(),
      owner: "Alice".to_string(),
      track_count: 1,
      id: Some("road-trip".to_string()),
      owner_id: None,
      collaborative: false,
      public: None,
      image_url: Some("https://example.test/playlist.jpg".to_string()),
      description: Some("Songs for the road".to_string()),
    }];
    *app.playlist_folder_items_mut() = vec![PlaylistFolderItem::Playlist {
      index: 0,
      current_id: 0,
    }];
    app.current_playlist_folder_id = 0;
    // Row 0 is "add playlist"; the community pin occupies display row 0.
    app.view.selected_playlist_index = Some(2);

    let area = Rect::new(0, 0, 60, 12);
    let mut terminal = Terminal::new(TestBackend::new(area.width, area.height)).unwrap();
    terminal
      .draw(|f| draw_browse_preview(f, &app, area))
      .unwrap();
    let buffer = terminal.backend().buffer();
    let content: String = (0..area.height)
      .flat_map(|y| (0..area.width).map(move |x| (x, y)))
      .filter_map(|(x, y)| buffer.cell((x, y)))
      .map(|cell| cell.symbol().to_string())
      .collect();

    for expected in [
      "Road Trip",
      "Description",
      "Songs for the road",
      "Author",
      "Alice",
    ] {
      assert!(
        content.contains(expected),
        "the playlist preview should show {expected}: {content}"
      );
    }
  }

  #[cfg(feature = "cover-art")]
  #[test]
  fn every_search_result_type_renders_preview_details() {
    use crate::core::app::BrowsePreviewItem;
    use crate::core::plugin_api::{AlbumInfo, ArtistInfo, ArtistRef, ShowInfo};
    use crate::core::test_helpers::playlist_info;

    let app = App::default();
    let strings = app.strings();
    let mut playlist = playlist_info("road-trip", "Road Trip", "Alice", false);
    playlist.description = Some("Songs for the road".to_string());
    playlist.track_count = 12;
    let field = |label: &str, value: &str| format!("{label}: {value}");
    let description = field(strings.screens.playlist_description, "Songs for the road");
    let author = field(strings.screens.playlist_author, "Alice");
    let playlist_tracks = field(strings.screens.browse_preview_tracks, "12");
    let artist = field(strings.screens.artist, "Artist Name");
    let album_artist = field(strings.screens.artist, "Album Artist");
    let album_type = field(
      strings.screens.browse_preview_type,
      &strings.screens.album_type(Some("album")),
    );
    let year = field(strings.screens.now_playing_year, "2024");
    let album_tracks = field(strings.screens.browse_preview_tracks, "11");
    let publisher = field(strings.screens.column_publisher, "Public Radio");
    let show_description = field(strings.screens.playlist_description, "Episodes about music");
    let items = [
      (
        BrowsePreviewItem::Playlist(playlist),
        vec![description, author, playlist_tracks],
      ),
      (
        BrowsePreviewItem::Artist(ArtistInfo {
          name: "Artist Name".to_string(),
          ..Default::default()
        }),
        vec![artist],
      ),
      (
        BrowsePreviewItem::Album(AlbumInfo {
          name: "Album Name".to_string(),
          artists: vec![ArtistRef {
            id: None,
            name: "Album Artist".to_string(),
          }],
          album_type: Some("album".to_string()),
          release_date: Some("2024-06-28".to_string()),
          total_tracks: Some(11),
          ..Default::default()
        }),
        vec![album_artist, album_type, year, album_tracks],
      ),
      (
        BrowsePreviewItem::Show(ShowInfo {
          name: "Show Name".to_string(),
          publisher: "Public Radio".to_string(),
          description: "Episodes about music".to_string(),
          ..Default::default()
        }),
        vec![publisher, show_description],
      ),
    ];

    for (item, expected_lines) in items {
      let rendered: Vec<String> = browse_preview_lines(&item, strings)
        .iter()
        .map(|line| {
          line
            .spans
            .iter()
            .map(|span| span.content.to_string())
            .collect::<String>()
        })
        .collect();
      let text = rendered.join(" | ");
      for expected in expected_lines {
        assert!(text.contains(&expected), "missing {expected} in {text}");
      }
    }
  }

  #[cfg(feature = "cover-art")]
  #[test]
  fn song_details_render_when_cover_art_is_disabled() {
    use crate::core::app::{ActiveBlock, RouteId, SearchResult, SearchResultBlock};

    let mut app = App::default();
    app.push_navigation_stack(RouteId::Search, ActiveBlock::SearchResultBlock);
    app.set_search_results(SearchResult {
      tracks: Some(Paged {
        items: vec![TrackInfo {
          name: "Song".to_string(),
          artists: vec!["Artist".to_string()],
          album: "Album".to_string(),
          duration_ms: 195_000,
          ..station("Song", vec![], "")
        }],
        total: 1,
        ..Default::default()
      }),
      ..Default::default()
    });
    app.select_search_result_row(SearchResultBlock::SongSearch, Some(0));
    app.user_config.behavior.draw_cover_art = false;
    app.user_config.behavior.draw_cover_art_forced = true;

    assert!(browse_preview_height(&app, Rect::new(0, 0, 90, 30)) > 0);
    let mut terminal = Terminal::new(TestBackend::new(60, 12)).unwrap();
    terminal
      .draw(|f| draw_browse_preview(f, &app, Rect::new(0, 0, 60, 12)))
      .unwrap();
    let buffer = terminal.backend().buffer();
    let content: String = (0..12)
      .flat_map(|y| (0..60).map(move |x| (x, y)))
      .filter_map(|(x, y)| buffer.cell((x, y)))
      .map(|cell| cell.symbol().to_string())
      .collect();

    for expected in ["Song", "Artist", "Album", "3:15"] {
      assert!(
        content.contains(expected),
        "the selected-song details should render without cover art: {content}"
      );
    }
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
