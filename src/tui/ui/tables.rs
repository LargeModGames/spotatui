use crate::core::app::{
  ActiveBlock, AlbumTableContext, App, EpisodeTableContext, RecommendationsContext,
};
use crate::core::plugin_api::{EpisodeInfo, SavedAlbumInfo, ShowInfo, TrackInfo};
use ratatui::{
  layout::{Constraint, Rect},
  style::{Modifier, Style},
  text::Span,
  widgets::{Block, Borders, Row, Table},
  Frame,
};
use rspotify::model::PlayableItem;
use rspotify::prelude::Id;

#[cfg(test)]
use super::columns::resolve_columns;
use super::columns::{resolve_columns_localized, ResolvedColumn, TableColumnSet};
use super::util::{
  get_color, get_percentage_width, join_artist_names, millis_to_minutes, release_year,
};
use crate::tui::theme::{EmphasisExt, ThemeExt};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum TableId {
  Album,
  AlbumList,
  Artist,
  Podcast,
  Song,
  RecentlyPlayed,
  PodcastEpisodes,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ColumnId {
  #[default]
  None,
  Title,
  Liked,
}

pub struct TableHeader {
  pub id: TableId,
  pub items: Vec<TableHeaderItem>,
}

impl TableHeader {
  pub fn get_index(&self, id: ColumnId) -> Option<usize> {
    self.items.iter().position(|item| item.id == id)
  }
}

#[derive(Default)]
pub struct TableHeaderItem {
  pub id: ColumnId,
  pub text: String,
  pub width: u16,
}

#[derive(Clone)]
pub struct TableItem {
  pub id: String,
  pub format: Vec<String>,
}

struct AlbumUi {
  selected_index: usize,
  items: Vec<TableItem>,
  title: String,
  offset: usize,
}

fn table_visible_rows(app: &App, layout_chunk: Rect) -> usize {
  // Clamp the configured padding to half the table height so an oversized
  // value can never zero out the visible-row count (which would pin the
  // scroll offset to 0 and make off-screen rows unreachable).
  let padding = app
    .user_config
    .behavior
    .table_scroll_padding
    .min(layout_chunk.height / 2);
  layout_chunk
    .height
    .checked_sub(padding)
    .map(|height| height as usize)
    .unwrap_or(0)
}

fn window_at<T>(offset: usize, layout_chunk: Rect, items: &[T]) -> (usize, &[T]) {
  // The scroll offset math works with fewer rows than the drawable area (that
  // gap is the scroll padding below the selection), but the table still
  // renders rows into it, so slice a full chunk-height of rows to fill every
  // line.
  let offset = offset.min(items.len());
  let end = items
    .len()
    .min(offset.saturating_add(layout_chunk.height as usize));
  (offset, &items[offset..end])
}

/// Slice a backing collection down to the rows that can be visible in this
/// layout, so callers only format the viewport instead of the whole
/// collection. Returns the scroll offset and the visible slice; the offset
/// math must stay in sync with what `draw_table` expects.
fn visible_window<'a, T>(
  app: &App,
  layout_chunk: Rect,
  selected_index: usize,
  items: &'a [T],
) -> (usize, &'a [T]) {
  let visible_rows = table_visible_rows(app, layout_chunk);
  window_at(
    table_scroll_offset(selected_index, visible_rows),
    layout_chunk,
    items,
  )
}

/// Like `visible_window`, but anchored to a persisted scroll offset: the
/// cursor moves within the visible rows without moving the view, and the view
/// scrolls only when the cursor reaches the top visible row (going up) or the
/// padded bottom row (going down). The updated offset is written back to
/// `scroll_offset` so the next frame and the mouse handler see it.
fn visible_window_anchored<'a, T>(
  app: &App,
  layout_chunk: Rect,
  selected_index: usize,
  scroll_offset: &std::cell::Cell<usize>,
  items: &'a [T],
) -> (usize, &'a [T]) {
  let visible_rows = table_visible_rows(app, layout_chunk);
  let max_cursor_row = visible_rows.saturating_sub(1);

  let anchor = scroll_offset.get().min(items.len().saturating_sub(1));
  let offset = if selected_index < anchor {
    selected_index
  } else if selected_index > anchor.saturating_add(max_cursor_row) {
    selected_index - max_cursor_row
  } else {
    anchor
  };
  scroll_offset.set(offset);

  window_at(offset, layout_chunk, items)
}

fn table_columns(app: &App, table: TableColumnSet, layout_width: u16) -> Vec<ResolvedColumn> {
  let configured = match table {
    TableColumnSet::Songs => &app.user_config.tables.songs,
    TableColumnSet::AlbumTracks => &app.user_config.tables.album_tracks,
    TableColumnSet::Albums => &app.user_config.tables.albums,
    TableColumnSet::Podcasts => &app.user_config.tables.podcasts,
    TableColumnSet::Episodes => &app.user_config.tables.episodes,
    TableColumnSet::RecentlyPlayed => &app.user_config.tables.recently_played,
  };
  resolve_columns_localized(table, layout_width, configured, &app.strings().screens)
}

fn table_header(id: TableId, columns: &[ResolvedColumn]) -> TableHeader {
  TableHeader {
    id,
    items: columns
      .iter()
      .map(|column| TableHeaderItem {
        id: column.column_id,
        text: column.header.clone(),
        width: column.width,
      })
      .collect(),
  }
}

fn track_table_item(
  track: &TrackInfo,
  columns: &[ResolvedColumn],
  strings: &crate::core::i18n::Strings,
) -> TableItem {
  TableItem {
    id: track.id.clone().unwrap_or_default(),
    format: columns
      .iter()
      .map(|column| match column.id.as_str() {
        "liked" => String::new(),
        "index" => track.track_number.to_string(),
        "title" => track.display_name(strings).to_owned(),
        "artist" => track.artists.join(", "),
        "album" => track.album.clone(),
        "length" => millis_to_minutes(track.duration_ms as u128),
        "year" => release_year(track.release_date.as_deref())
          .unwrap_or_default()
          .to_string(),
        _ => String::new(),
      })
      .collect(),
  }
}

fn album_table_item(album: &SavedAlbumInfo, columns: &[ResolvedColumn], app: &App) -> TableItem {
  let has_liked_column = columns.iter().any(|column| column.id == "liked");
  TableItem {
    id: album.album.id.clone().unwrap_or_default(),
    format: columns
      .iter()
      .map(|column| match column.id.as_str() {
        "liked" => app.user_config.padded_liked_icon(),
        "title" => {
          if has_liked_column {
            album.album.name.clone()
          } else {
            format!(
              "{}{}",
              app.user_config.padded_liked_icon(),
              album.album.name
            )
          }
        }
        "artist" => join_artist_names(&album.album.artists),
        "date" => album.album.release_date.clone().unwrap_or_default(),
        _ => String::new(),
      })
      .collect(),
  }
}

fn podcast_table_item(show: &ShowInfo, columns: &[ResolvedColumn]) -> TableItem {
  TableItem {
    id: show.id.clone().unwrap_or_default(),
    format: columns
      .iter()
      .map(|column| match column.id.as_str() {
        "title" => show.name.clone(),
        "publisher" => show.publisher.clone(),
        _ => String::new(),
      })
      .collect(),
  }
}

fn episode_table_item(episode: &EpisodeInfo, columns: &[ResolvedColumn], app: &App) -> TableItem {
  let duration_ms = episode.duration_ms as u128;
  let (played_str, time_str) = match &episode.resume_point {
    Some(resume_point) => (
      if resume_point.fully_played {
        format!(" {}", app.user_config.behavior.episode_played_icon)
      } else {
        String::new()
      },
      format!(
        "{} / {}",
        millis_to_minutes(resume_point.resume_position_ms as u128),
        millis_to_minutes(duration_ms)
      ),
    ),
    None => (String::new(), millis_to_minutes(duration_ms)),
  };

  TableItem {
    id: episode.id.clone().unwrap_or_default(),
    format: columns
      .iter()
      .map(|column| match column.id.as_str() {
        "played" => played_str.clone(),
        "date" => episode.release_date.clone(),
        "title" => episode.name.clone(),
        "duration" => time_str.clone(),
        _ => String::new(),
      })
      .collect(),
  }
}

/// Render the Local Files folder browser: a selectable list of folders (one per
/// subdirectory of the configured music directory), each with its track count.
pub fn draw_local_browser(f: &mut Frame<'_>, app: &App, layout_chunk: Rect) {
  let current_route = app.get_current_route();
  let highlight_state = (
    current_route.active_block == ActiveBlock::LocalBrowser,
    current_route.hovered_block == ActiveBlock::LocalBrowser,
  );

  let items: Vec<String> = app
    .local_playlists()
    .iter()
    .map(|folder| {
      if folder.track_count > 0 {
        app
          .strings()
          .screens
          .folder_tracks(folder.display_name(app.strings()), folder.track_count)
      } else {
        folder.display_name(app.strings()).to_string()
      }
    })
    .collect();

  let title = if app.local_playlists().is_empty() {
    app.strings().screens.local_files_empty
  } else {
    app.strings().screens.local_files
  };

  super::util::draw_selectable_list(
    f,
    app,
    layout_chunk,
    title,
    &items,
    highlight_state,
    Some(app.view.local_playlists_index),
    &app.view.local_files_scroll_offset,
  );
}

pub fn draw_artist_table(f: &mut Frame<'_>, app: &App, layout_chunk: Rect) {
  let header = TableHeader {
    id: TableId::Artist,
    items: vec![TableHeaderItem {
      text: app.strings().screens.artist.to_string(),
      width: get_percentage_width(layout_chunk.width, 1.0),
      ..Default::default()
    }],
  };

  let current_route = app.get_current_route();
  let highlight_state = (
    current_route.active_block == ActiveBlock::Artists,
    current_route.hovered_block == ActiveBlock::Artists,
  );

  // Directly calculate window over all continuous artists:
  let (offset, visible) =
    visible_window(app, layout_chunk, app.view.artists_list_index, &app.artists);
  let items = visible
    .iter()
    .map(|item| TableItem {
      id: item.id.clone().unwrap_or_default(),
      format: vec![item.name.to_owned()],
    })
    .collect::<Vec<TableItem>>();

  draw_table(
    f,
    app,
    layout_chunk,
    (app.strings().screens.artists, &header),
    &items,
    app.view.artists_list_index,
    offset,
    highlight_state,
  );
}

pub fn draw_podcast_table(f: &mut Frame<'_>, app: &App, layout_chunk: Rect) {
  let columns = table_columns(app, TableColumnSet::Podcasts, layout_chunk.width);
  let header = table_header(TableId::Podcast, &columns);

  let current_route = app.get_current_route();

  let highlight_state = (
    current_route.active_block == ActiveBlock::Podcasts,
    current_route.hovered_block == ActiveBlock::Podcasts,
  );

  if let Some(saved_shows) = app.library().saved_shows.get_results(None) {
    let (offset, visible) = visible_window(
      app,
      layout_chunk,
      app.view.shows_list_index,
      &saved_shows.items,
    );
    let items = visible
      .iter()
      .map(|show| podcast_table_item(show, &columns))
      .collect::<Vec<TableItem>>();

    draw_table(
      f,
      app,
      layout_chunk,
      (app.strings().screens.podcasts, &header),
      &items,
      app.view.shows_list_index,
      offset,
      highlight_state,
    )
  } else {
    draw_table(
      f,
      app,
      layout_chunk,
      (app.strings().screens.podcasts, &header),
      &[],
      app.view.shows_list_index,
      0,
      highlight_state,
    )
  }
}

pub fn draw_album_table(f: &mut Frame<'_>, app: &App, layout_chunk: Rect) {
  let columns = table_columns(app, TableColumnSet::AlbumTracks, layout_chunk.width);
  let header = table_header(TableId::Album, &columns);

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
        .map(|selected_album_simplified| {
          let (offset, visible) = visible_window(
            app,
            layout_chunk,
            selected_album_simplified.selected_index,
            &selected_album_simplified.tracks.items,
          );
          AlbumUi {
            items: visible
              .iter()
              .map(|item| track_table_item(item, &columns, app.strings()))
              .collect::<Vec<TableItem>>(),
            title: app.strings().screens.title_by_artist(
              &selected_album_simplified.album.name,
              &join_artist_names(&selected_album_simplified.album.artists),
            ),
            selected_index: selected_album_simplified.selected_index,
            offset,
          }
        })
    }
    AlbumTableContext::Full => app.selected_album_full.as_ref().map(|selected_album| {
      let (offset, visible) = visible_window(
        app,
        layout_chunk,
        app.view.saved_album_tracks_index,
        &selected_album.album.tracks,
      );
      AlbumUi {
        items: visible
          .iter()
          .map(|item| track_table_item(item, &columns, app.strings()))
          .collect::<Vec<TableItem>>(),
        title: app.strings().screens.title_by_artist(
          &selected_album.album.name,
          &join_artist_names(&selected_album.album.artists),
        ),
        selected_index: app.view.saved_album_tracks_index,
        offset,
      }
    }),
  };

  if let Some(album_ui) = album_ui {
    draw_table(
      f,
      app,
      layout_chunk,
      (&album_ui.title, &header),
      &album_ui.items,
      album_ui.selected_index,
      album_ui.offset,
      highlight_state,
    );
  };
}

pub fn draw_recommendations_table(f: &mut Frame<'_>, app: &App, layout_chunk: Rect) {
  let columns = table_columns(app, TableColumnSet::Songs, layout_chunk.width);
  let header = table_header(TableId::Song, &columns);

  let current_route = app.get_current_route();
  let highlight_state = (
    current_route.active_block == ActiveBlock::TrackTable,
    current_route.hovered_block == ActiveBlock::TrackTable,
  );

  let (offset, visible) = visible_window_anchored(
    app,
    layout_chunk,
    app.view.track_table_index,
    &app.view.track_table_scroll_offset,
    &app.track_table.tracks,
  );
  let items = visible
    .iter()
    .map(|item| track_table_item(item, &columns, app.strings()))
    .collect::<Vec<TableItem>>();
  // match RecommendedContext
  let recommendations_ui = match &app.recommendations_context {
    Some(RecommendationsContext::Song) => app
      .strings()
      .screens
      .recommendations_song(&app.recommendations_seed),
    Some(RecommendationsContext::Artist) => app
      .strings()
      .screens
      .recommendations_artist(&app.recommendations_seed),
    None => app.strings().screens.recommendations.to_string(),
  };
  draw_table(
    f,
    app,
    layout_chunk,
    (&recommendations_ui[..], &header),
    &items,
    app.view.track_table_index,
    offset,
    highlight_state,
  )
}

pub fn draw_song_table(f: &mut Frame<'_>, app: &App, layout_chunk: Rect) {
  let columns = table_columns(app, TableColumnSet::Songs, layout_chunk.width);
  let header = table_header(TableId::Song, &columns);

  let current_route = app.get_current_route();
  let highlight_state = (
    current_route.active_block == ActiveBlock::TrackTable,
    current_route.hovered_block == ActiveBlock::TrackTable,
  );

  let (offset, visible) = visible_window_anchored(
    app,
    layout_chunk,
    app.view.track_table_index,
    &app.view.track_table_scroll_offset,
    &app.track_table.tracks,
  );
  let items = visible
    .iter()
    .map(|item| track_table_item(item, &columns, app.strings()))
    .collect::<Vec<TableItem>>();

  let title = if app.is_playlist_track_table_context() {
    if let Some(query) = app.pending_playlist_track_search.as_ref() {
      app.strings().screens.songs_searching(query)
    } else {
      app
        .active_playlist_track_filter
        .as_ref()
        .map(|query| app.strings().screens.songs_filtered(query))
        .unwrap_or_else(|| app.strings().screens.songs.to_string())
    }
  } else {
    app.strings().screens.songs.to_string()
  };

  draw_table(
    f,
    app,
    layout_chunk,
    (&title, &header),
    &items,
    app.view.track_table_index,
    offset,
    highlight_state,
  )
}

pub fn draw_album_list(f: &mut Frame<'_>, app: &App, layout_chunk: Rect) {
  let columns = table_columns(app, TableColumnSet::Albums, layout_chunk.width);
  let header = table_header(TableId::AlbumList, &columns);

  let current_route = app.get_current_route();

  let highlight_state = (
    current_route.active_block == ActiveBlock::AlbumList,
    current_route.hovered_block == ActiveBlock::AlbumList,
  );

  let selected_song_index = app.view.album_list_index;

  if let Some(saved_albums) = app.library().saved_albums.get_results(None) {
    let (offset, visible) =
      visible_window(app, layout_chunk, selected_song_index, &saved_albums.items);
    let items = visible
      .iter()
      .map(|saved_album| album_table_item(saved_album, &columns, app))
      .collect::<Vec<TableItem>>();

    draw_table(
      f,
      app,
      layout_chunk,
      (app.strings().screens.saved_albums, &header),
      &items,
      selected_song_index,
      offset,
      highlight_state,
    )
  } else {
    draw_table(
      f,
      app,
      layout_chunk,
      (app.strings().screens.saved_albums, &header),
      &[],
      selected_song_index,
      0,
      highlight_state,
    )
  }
}

pub fn draw_show_episodes(f: &mut Frame<'_>, app: &App, layout_chunk: Rect) {
  let columns = table_columns(app, TableColumnSet::Episodes, layout_chunk.width);
  let header = table_header(TableId::PodcastEpisodes, &columns);

  let current_route = app.get_current_route();

  let highlight_state = (
    current_route.active_block == ActiveBlock::EpisodeTable,
    current_route.hovered_block == ActiveBlock::EpisodeTable,
  );

  let title = match &app.episode_table_context {
    EpisodeTableContext::Simplified => match &app.selected_show_simplified {
      Some(selected_show) => app
        .strings()
        .screens
        .title_by_artist(&selected_show.show.name, &selected_show.show.publisher),
      None => app.strings().screens.episodes.to_owned(),
    },
    EpisodeTableContext::Full => match &app.selected_show_full {
      Some(selected_show) => app
        .strings()
        .screens
        .title_by_artist(&selected_show.show.name, &selected_show.show.publisher),
      None => app.strings().screens.episodes.to_owned(),
    },
  };

  if let Some(episodes) = app.library().show_episodes.get_results(None) {
    let (offset, visible) = visible_window(
      app,
      layout_chunk,
      app.view.episode_list_index,
      &episodes.items,
    );
    let items = visible
      .iter()
      .map(|episode| episode_table_item(episode, &columns, app))
      .collect::<Vec<TableItem>>();

    draw_table(
      f,
      app,
      layout_chunk,
      (&title, &header),
      &items,
      app.view.episode_list_index,
      offset,
      highlight_state,
    );
  } else {
    draw_table(
      f,
      app,
      layout_chunk,
      (&title, &header),
      &[],
      app.view.episode_list_index,
      0,
      highlight_state,
    );
  }
}

pub fn draw_recently_played_table(f: &mut Frame<'_>, app: &App, layout_chunk: Rect) {
  let columns = table_columns(app, TableColumnSet::RecentlyPlayed, layout_chunk.width);
  let header = table_header(TableId::RecentlyPlayed, &columns);

  let current_route = app.get_current_route();

  let highlight_state = (
    current_route.active_block == ActiveBlock::RecentlyPlayed,
    current_route.hovered_block == ActiveBlock::RecentlyPlayed,
  );

  let selected_song_index = app.view.recently_played_index;

  if let Some(recently_played) = &app.recently_played {
    let (offset, visible) = visible_window(
      app,
      layout_chunk,
      selected_song_index,
      &recently_played.items,
    );
    let items = visible
      .iter()
      .map(|item| track_table_item(item, &columns, app.strings()))
      .collect::<Vec<TableItem>>();

    draw_table(
      f,
      app,
      layout_chunk,
      (app.strings().screens.recent_tracks, &header),
      &items,
      selected_song_index,
      offset,
      highlight_state,
    )
  } else {
    draw_table(
      f,
      app,
      layout_chunk,
      (app.strings().screens.recent_tracks, &header),
      &[],
      selected_song_index,
      0,
      highlight_state,
    )
  }
}

#[allow(clippy::too_many_arguments)]
fn draw_table(
  f: &mut Frame<'_>,
  app: &App,
  layout_chunk: Rect,
  table_layout: (&str, &TableHeader), // (title, header colums)
  items: &[TableItem], // Visible window only (see `visible_window`); same length as `header_columns`
  selected_index: usize,
  offset: usize, // Index of `items[0]` within the full backing collection
  highlight_state: (bool, bool),
) {
  let selected_style = get_color(highlight_state, app.user_config.theme)
    .add_modifier(Modifier::REVERSED | app.user_config.behavior.emphasis(Modifier::BOLD));

  let track_playing_index = app.current_playback_context.as_ref().and_then(|ctx| {
    ctx.item.as_ref().and_then(|item| match item {
      PlayableItem::Track(track) => {
        let track_id_str = track.id.as_ref().map(|id| id.id().to_string());
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
      _ => None,
    })
  });

  let (title, header) = table_layout;

  let rows = items.iter().enumerate().map(|(i, item)| {
    let mut formatted_row = item.format.clone();
    let mut style = app.user_config.theme.base_style(); // default styling

    // if table displays songs
    match header.id {
      TableId::Song | TableId::RecentlyPlayed | TableId::Album => {
        // First check if the song should be highlighted because it is currently playing.
        // The marker goes on the title cell, falling back to the first cell when the
        // user's column config omits `title` so the playing row is never unmarked.
        if track_playing_index == Some(i) {
          let title_idx = header.get_index(ColumnId::Title).unwrap_or(0);
          if let Some(cell) = formatted_row.get_mut(title_idx) {
            cell.insert_str(0, &app.user_config.padded_playing_icon());
          }
          style = Style::default()
            .fg(app.user_config.theme.active.into())
            .add_modifier(app.user_config.behavior.emphasis(Modifier::BOLD));
        }

        // Show this the liked icon if the song is liked
        if let Some(liked_idx) = header.get_index(ColumnId::Liked) {
          if app.liked_song_ids_set().contains(item.id.as_str()) {
            formatted_row[liked_idx] = app.user_config.padded_liked_icon();
          }
        }
      }
      TableId::PodcastEpisodes if track_playing_index == Some(i) => {
        let name_idx = header.get_index(ColumnId::Title).unwrap_or(0);
        if let Some(cell) = formatted_row.get_mut(name_idx) {
          cell.insert_str(0, &app.user_config.padded_playing_icon());
        }
        style = Style::default()
          .fg(app.user_config.theme.active.into())
          .add_modifier(app.user_config.behavior.emphasis(Modifier::BOLD));
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
      Row::new(header.items.iter().map(|h| h.text.as_str()))
        .style(Style::default().fg(app.user_config.theme.header.into())),
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

pub fn table_scroll_offset(selected_index: usize, visible_rows: usize) -> usize {
  if visible_rows == 0 {
    return 0;
  }

  selected_index.saturating_sub(visible_rows.saturating_sub(1))
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::plugin_api::PlaylistInfo;
  use ratatui::{backend::TestBackend, Terminal};

  fn rendered(app: &App, area: Rect) -> String {
    rendered_with(app, area, draw_local_browser)
  }

  fn rendered_with(app: &App, area: Rect, draw: fn(&mut Frame<'_>, &App, Rect)) -> String {
    let mut terminal = Terminal::new(TestBackend::new(area.width, area.height)).unwrap();
    terminal.draw(|f| draw(f, app, area)).unwrap();
    let buffer = terminal.backend().buffer();
    (0..area.height)
      .flat_map(|y| (0..area.width).map(move |x| (x, y)))
      .filter_map(|(x, y)| buffer.cell((x, y)).map(|c| c.symbol().to_string()))
      .collect()
  }

  fn folder(name: &str, track_count: u32) -> PlaylistInfo {
    PlaylistInfo {
      generated_name: None,
      uri: format!("file:///music/{name}"),
      name: name.to_string(),
      owner: "local".to_string(),
      track_count,
      id: None,
      owner_id: None,
      collaborative: false,
      public: None,
      image_url: None,
    }
  }

  fn track(i: u32) -> TrackInfo {
    TrackInfo {
      generated_name: None,
      uri: Some(format!("spotify:track:{i:022}")),
      name: format!("Track {i}"),
      artists: vec![format!("Artist {i}")],
      album: format!("Album {i}"),
      duration_ms: 180_000,
      id: Some(format!("{i:022}")),
      album_id: None,
      artist_refs: Vec::new(),
      is_playable: true,
      is_local: false,
      track_number: i + 1,
      explicit: false,
      image_url: None,
      release_date: None,
    }
  }

  fn column(id: &str) -> crate::core::user_config::ColumnSpec {
    crate::core::user_config::ColumnSpec {
      id: id.to_string(),
      ..Default::default()
    }
  }

  fn dated(release_date: Option<&str>) -> TrackInfo {
    TrackInfo {
      release_date: release_date.map(str::to_string),
      ..track(1)
    }
  }

  #[test]
  fn the_year_column_renders_the_album_release_year() {
    let columns = resolve_columns(
      TableColumnSet::Songs,
      80,
      &[column("title"), column("year")],
    );
    for date in ["1997", "1997-01", "1997-01-20"] {
      assert_eq!(
        track_table_item(&dated(Some(date)), &columns, &crate::core::i18n::EN).format,
        ["Track 1", "1997"],
        "{date}"
      );
    }
  }

  #[test]
  fn the_year_column_is_empty_for_an_unknown_release_date() {
    let columns = resolve_columns(TableColumnSet::Songs, 80, &[column("year")]);
    for date in [None, Some(""), Some("0000")] {
      assert_eq!(
        track_table_item(&dated(date), &columns, &crate::core::i18n::EN).format,
        [""],
        "{date:?}"
      );
    }
  }

  #[test]
  fn a_configured_year_column_shows_in_the_song_table() {
    let mut app = App::default();
    app.user_config.tables.songs = vec![column("title"), column("year")];
    app.track_table.tracks = vec![dated(Some("1997-01-20"))];

    let content = rendered_with(&app, Rect::new(0, 0, 60, 6), draw_song_table);
    assert!(content.contains("Year"), "header: {content}");
    assert!(content.contains("1997"), "row: {content}");
  }

  #[test]
  fn song_table_fills_rows_below_scroll_padding() {
    let mut app = App::default();
    app.track_table.tracks = (0..30).map(track).collect();
    app.view.track_table_index = 15;

    // Height 12 with the default scroll padding of 5: the scroll offset math
    // treats 7 rows as visible, but the drawable area holds 9 data rows
    // (12 minus 2 borders and 1 header). The rows past the padding window
    // must still be filled with the following tracks, not left blank.
    let area = Rect::new(0, 0, 80, 12);
    let mut terminal = Terminal::new(TestBackend::new(area.width, area.height)).unwrap();
    terminal.draw(|f| draw_song_table(f, &app, area)).unwrap();
    let buffer = terminal.backend().buffer();
    let content: String = (0..area.height)
      .flat_map(|y| (0..area.width).map(move |x| (x, y)))
      .filter_map(|(x, y)| buffer.cell((x, y)).map(|c| c.symbol().to_string()))
      .collect();

    for i in [15, 16, 17] {
      assert!(
        content.contains(&format!("Track {i}")),
        "row for track {i} should be rendered: {content}"
      );
    }
    assert!(
      !content.contains("Track 18"),
      "track 18 is below the drawable area and should not be rendered: {content}"
    );
  }

  #[test]
  fn song_table_view_stays_anchored_until_cursor_hits_top() {
    let mut app = App::default();
    app.track_table.tracks = (0..30).map(track).collect();

    let area = Rect::new(0, 0, 80, 12);
    let mut terminal = Terminal::new(TestBackend::new(area.width, area.height)).unwrap();
    let first_data_row = |terminal: &Terminal<TestBackend>| -> String {
      let buffer = terminal.backend().buffer();
      (0..area.width)
        .filter_map(|x| buffer.cell((x, 2)).map(|c| c.symbol().to_string()))
        .collect()
    };

    // Scroll down to row 15: with height 12 and default padding 5 the offset
    // math treats 7 rows as visible, so the view lands at offset 9.
    app.view.track_table_index = 15;
    terminal.draw(|f| draw_song_table(f, &app, area)).unwrap();
    assert!(
      first_data_row(&terminal).contains("Track 9"),
      "view should scroll to offset 9"
    );

    // Moving the cursor back up within the window must NOT move the view.
    app.view.track_table_index = 12;
    terminal.draw(|f| draw_song_table(f, &app, area)).unwrap();
    assert!(
      first_data_row(&terminal).contains("Track 9"),
      "view should stay anchored while the cursor moves inside the window"
    );

    // At the top visible row the view still holds...
    app.view.track_table_index = 9;
    terminal.draw(|f| draw_song_table(f, &app, area)).unwrap();
    assert!(
      first_data_row(&terminal).contains("Track 9"),
      "cursor on the top visible row should not scroll yet"
    );

    // ...and only going past it scrolls the view up.
    app.view.track_table_index = 8;
    terminal.draw(|f| draw_song_table(f, &app, area)).unwrap();
    assert!(
      first_data_row(&terminal).contains("Track 8"),
      "going above the top visible row should scroll the view up"
    );
  }

  #[test]
  fn local_browser_lists_folders_with_track_counts() {
    let mut app = App::default();
    *app.local_playlists_mut() = vec![folder("MyAlbum", 3)];
    let content = rendered(&app, Rect::new(0, 0, 60, 6));
    assert!(
      content.contains("MyAlbum (3 tracks)"),
      "folder name and track count should render: {content}"
    );
  }

  #[test]
  fn local_browser_says_track_for_a_one_track_folder() {
    let mut app = App::default();
    *app.local_playlists_mut() = vec![folder("Single", 1)];
    let content = rendered(&app, Rect::new(0, 0, 60, 6));
    assert!(
      content.contains("Single (1 track)"),
      "a one-track folder should use the singular: {content}"
    );
  }

  #[test]
  fn local_browser_empty_shows_config_hint() {
    let app = App::default();
    let content = rendered(&app, Rect::new(0, 0, 80, 6));
    assert!(
      content.contains("local_music_path"),
      "empty browser should hint at the config key: {content}"
    );
  }

  fn assert_empty_titled_table(draw: fn(&mut Frame<'_>, &App, Rect), title: &str) {
    let app = App::default();
    let content = rendered_with(&app, Rect::new(0, 0, 80, 8), draw);
    assert!(
      content.contains(title),
      "pane should show the {title:?} title before data loads: {content}"
    );
  }

  #[test]
  fn album_list_draws_an_empty_titled_table_before_data_loads() {
    assert_empty_titled_table(draw_album_list, "Saved Albums");
  }

  #[test]
  fn podcast_table_draws_an_empty_titled_table_before_data_loads() {
    assert_empty_titled_table(draw_podcast_table, "Podcasts");
  }

  #[test]
  fn recently_played_draws_an_empty_titled_table_before_data_loads() {
    assert_empty_titled_table(draw_recently_played_table, "Recently Played Tracks");
  }

  #[test]
  fn show_episodes_draws_an_empty_titled_table_before_data_loads() {
    assert_empty_titled_table(draw_show_episodes, "Episodes");
  }
  #[test]
  fn cached_generated_track_titles_follow_language_in_table_cells_without_renaming_user_tracks() {
    use crate::core::i18n::Language;
    use crate::core::plugin_api::GeneratedTrackName;
    let mut app = App::default();
    let mut generated = track(0);
    generated.name = "Unknown Track".into();
    generated.generated_name = Some(GeneratedTrackName::LocalUnknown);
    let mut user = generated.clone();
    user.generated_name = None;
    let columns = resolve_columns(TableColumnSet::Songs, 80, &[column("title")]);
    for language in [Language::En, Language::ZhHans, Language::ZhHant] {
      app.user_config.behavior.language = language;
      app.refresh_language();
      assert_eq!(
        track_table_item(&generated, &columns, app.strings()).format,
        [app.strings().status.infra.local_unknown_track]
      );
      assert_eq!(
        track_table_item(&user, &columns, app.strings()).format,
        ["Unknown Track"]
      );
      assert_eq!(generated.name, "Unknown Track");
    }
  }

  /// The modifier of the first cell of `text` on row `y` once `draw` renders `app`.
  fn modifier_at(app: &App, draw: fn(&mut Frame<'_>, &App, Rect), y: u16, text: &str) -> Modifier {
    let area = Rect::new(0, 0, 60, 8);
    let mut terminal = Terminal::new(TestBackend::new(area.width, area.height)).unwrap();
    terminal.draw(|f| draw(f, app, area)).unwrap();
    let buffer = terminal.backend().buffer();
    let row: String = (0..area.width)
      .filter_map(|x| buffer.cell((x, y)).map(|c| c.symbol().to_string()))
      .collect();
    let x = row
      .find(text)
      .unwrap_or_else(|| panic!("{text} on row {y}: {row}"));
    let x = row[..x].chars().count() as u16;
    buffer.cell((x, y)).unwrap().modifier
  }

  fn song_table(text_emphasis: bool) -> App {
    let mut app = App::default();
    app.user_config.behavior.enable_text_emphasis = text_emphasis;
    app.track_table.tracks = (0..3).map(track).collect();
    app
  }

  #[test]
  fn selected_table_row_is_not_bold_when_text_emphasis_is_off() {
    let modifier = modifier_at(&song_table(false), draw_song_table, 2, "Track 0");
    assert!(modifier.contains(Modifier::REVERSED), "{modifier:?}");
    assert!(!modifier.contains(Modifier::BOLD), "{modifier:?}");
  }

  #[test]
  fn selected_table_row_is_bold_when_text_emphasis_is_on() {
    let modifier = modifier_at(&song_table(true), draw_song_table, 2, "Track 0");
    assert!(modifier.contains(Modifier::REVERSED), "{modifier:?}");
    assert!(modifier.contains(Modifier::BOLD), "{modifier:?}");
  }

  #[test]
  fn selected_list_row_is_not_bold_when_text_emphasis_is_off() {
    let mut app = App::default();
    app.user_config.behavior.enable_text_emphasis = false;
    *app.local_playlists_mut() = vec![folder("MyAlbum", 3)];
    let modifier = modifier_at(&app, draw_local_browser, 1, "MyAlbum");
    assert!(modifier.contains(Modifier::REVERSED), "{modifier:?}");
    assert!(!modifier.contains(Modifier::BOLD), "{modifier:?}");
  }
}
