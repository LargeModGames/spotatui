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

use super::columns::{resolve_columns, ResolvedColumn, TableColumnSet};
use super::util::{get_color, get_percentage_width, join_artist_names, millis_to_minutes};

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
  resolve_columns(table, layout_width, configured)
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

fn track_table_item(track: &TrackInfo, columns: &[ResolvedColumn]) -> TableItem {
  TableItem {
    id: track.id.clone().unwrap_or_default(),
    format: columns
      .iter()
      .map(|column| match column.id.as_str() {
        "liked" => String::new(),
        "index" => track.track_number.to_string(),
        "title" => track.name.clone(),
        "artist" => track.artists.join(", "),
        "album" => track.album.clone(),
        "length" => millis_to_minutes(track.duration_ms as u128),
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
    .local_playlists
    .iter()
    .map(|folder| {
      if folder.track_count > 0 {
        format!("{} ({} tracks)", folder.name, folder.track_count)
      } else {
        folder.name.clone()
      }
    })
    .collect();

  let title = if app.local_playlists.is_empty() {
    "Local Files (no folders — set behavior.local_music_path)"
  } else {
    "Local Files"
  };

  super::util::draw_selectable_list(
    f,
    app,
    layout_chunk,
    title,
    &items,
    highlight_state,
    Some(app.local_playlists_index),
  );
}

pub fn draw_artist_table(f: &mut Frame<'_>, app: &App, layout_chunk: Rect) {
  let header = TableHeader {
    id: TableId::Artist,
    items: vec![TableHeaderItem {
      text: "Artist".to_string(),
      width: get_percentage_width(layout_chunk.width, 1.0),
      ..Default::default()
    }],
  };

  let current_route = app.get_current_route();
  let highlight_state = (
    current_route.active_block == ActiveBlock::Artists,
    current_route.hovered_block == ActiveBlock::Artists,
  );

  if let Some(saved_artists) = app.library.saved_artists.get_results(None) {
    let (offset, visible) = visible_window(
      app,
      layout_chunk,
      app.artists_list_index,
      &saved_artists.items,
    );
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
      ("Artists", &header),
      &items,
      app.artists_list_index,
      offset,
      highlight_state,
    )
  } else {
    draw_table(
      f,
      app,
      layout_chunk,
      ("Artists", &header),
      &[],
      app.artists_list_index,
      0,
      highlight_state,
    )
  }
}

pub fn draw_podcast_table(f: &mut Frame<'_>, app: &App, layout_chunk: Rect) {
  let columns = table_columns(app, TableColumnSet::Podcasts, layout_chunk.width);
  let header = table_header(TableId::Podcast, &columns);

  let current_route = app.get_current_route();

  let highlight_state = (
    current_route.active_block == ActiveBlock::Podcasts,
    current_route.hovered_block == ActiveBlock::Podcasts,
  );

  if let Some(saved_shows) = app.library.saved_shows.get_results(None) {
    let (offset, visible) =
      visible_window(app, layout_chunk, app.shows_list_index, &saved_shows.items);
    let items = visible
      .iter()
      .map(|show| podcast_table_item(show, &columns))
      .collect::<Vec<TableItem>>();

    draw_table(
      f,
      app,
      layout_chunk,
      ("Podcasts", &header),
      &items,
      app.shows_list_index,
      offset,
      highlight_state,
    )
  };
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
              .map(|item| track_table_item(item, &columns))
              .collect::<Vec<TableItem>>(),
            title: format!(
              "{} by {}",
              selected_album_simplified.album.name,
              join_artist_names(&selected_album_simplified.album.artists)
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
        app.saved_album_tracks_index,
        &selected_album.album.tracks,
      );
      AlbumUi {
        items: visible
          .iter()
          .map(|item| track_table_item(item, &columns))
          .collect::<Vec<TableItem>>(),
        title: format!(
          "{} by {}",
          selected_album.album.name,
          join_artist_names(&selected_album.album.artists)
        ),
        selected_index: app.saved_album_tracks_index,
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
    app.track_table.selected_index,
    &app.track_table.scroll_offset,
    &app.track_table.tracks,
  );
  let items = visible
    .iter()
    .map(|item| track_table_item(item, &columns))
    .collect::<Vec<TableItem>>();
  // match RecommendedContext
  let recommendations_ui = match &app.recommendations_context {
    Some(RecommendationsContext::Song) => format!(
      "Recommendations based on Song \'{}\'",
      app.recommendations_seed
    ),
    Some(RecommendationsContext::Artist) => format!(
      "Recommendations based on Artist \'{}\'",
      app.recommendations_seed
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
    app.track_table.selected_index,
    &app.track_table.scroll_offset,
    &app.track_table.tracks,
  );
  let items = visible
    .iter()
    .map(|item| track_table_item(item, &columns))
    .collect::<Vec<TableItem>>();

  let title = if app.is_playlist_track_table_context() {
    if let Some(query) = app.pending_playlist_track_search.as_ref() {
      format!("Songs (searching: {query}...)")
    } else {
      app
        .active_playlist_track_filter
        .as_ref()
        .map(|query| format!("Songs (filtered: {query})"))
        .unwrap_or_else(|| "Songs".to_string())
    }
  } else {
    "Songs".to_string()
  };

  draw_table(
    f,
    app,
    layout_chunk,
    (&title, &header),
    &items,
    app.track_table.selected_index,
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

  let selected_song_index = app.album_list_index;

  if let Some(saved_albums) = app.library.saved_albums.get_results(None) {
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
      ("Saved Albums", &header),
      &items,
      selected_song_index,
      offset,
      highlight_state,
    )
  };
}

pub fn draw_show_episodes(f: &mut Frame<'_>, app: &App, layout_chunk: Rect) {
  let columns = table_columns(app, TableColumnSet::Episodes, layout_chunk.width);
  let header = table_header(TableId::PodcastEpisodes, &columns);

  let current_route = app.get_current_route();

  let highlight_state = (
    current_route.active_block == ActiveBlock::EpisodeTable,
    current_route.hovered_block == ActiveBlock::EpisodeTable,
  );

  if let Some(episodes) = app.library.show_episodes.get_results(None) {
    let (offset, visible) =
      visible_window(app, layout_chunk, app.episode_list_index, &episodes.items);
    let items = visible
      .iter()
      .map(|episode| episode_table_item(episode, &columns, app))
      .collect::<Vec<TableItem>>();

    let title = match &app.episode_table_context {
      EpisodeTableContext::Simplified => match &app.selected_show_simplified {
        Some(selected_show) => {
          format!(
            "{} by {}",
            selected_show.show.name, selected_show.show.publisher
          )
        }
        None => "Episodes".to_owned(),
      },
      EpisodeTableContext::Full => match &app.selected_show_full {
        Some(selected_show) => {
          format!(
            "{} by {}",
            selected_show.show.name, selected_show.show.publisher
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
      offset,
      highlight_state,
    );
  };
}

pub fn draw_recently_played_table(f: &mut Frame<'_>, app: &App, layout_chunk: Rect) {
  let columns = table_columns(app, TableColumnSet::RecentlyPlayed, layout_chunk.width);
  let header = table_header(TableId::RecentlyPlayed, &columns);

  if let Some(recently_played) = &app.recently_played.result {
    let current_route = app.get_current_route();

    let highlight_state = (
      current_route.active_block == ActiveBlock::RecentlyPlayed,
      current_route.hovered_block == ActiveBlock::RecentlyPlayed,
    );

    let selected_song_index = app.recently_played.index;

    let (offset, visible) = visible_window(
      app,
      layout_chunk,
      selected_song_index,
      &recently_played.items,
    );
    let items = visible
      .iter()
      .map(|item| track_table_item(item, &columns))
      .collect::<Vec<TableItem>>();

    draw_table(
      f,
      app,
      layout_chunk,
      ("Recently Played Tracks", &header),
      &items,
      selected_song_index,
      offset,
      highlight_state,
    )
  };
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
    .add_modifier(Modifier::BOLD | Modifier::REVERSED);

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
            .fg(app.user_config.theme.active)
            .add_modifier(app.user_config.behavior.emphasis(Modifier::BOLD));
        }

        // Show this the liked icon if the song is liked
        if let Some(liked_idx) = header.get_index(ColumnId::Liked) {
          if app.liked_song_ids_set.contains(item.id.as_str()) {
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
          .fg(app.user_config.theme.active)
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
    let mut terminal = Terminal::new(TestBackend::new(area.width, area.height)).unwrap();
    terminal.draw(|f| draw_local_browser(f, app, area)).unwrap();
    let buffer = terminal.backend().buffer();
    (0..area.height)
      .flat_map(|y| (0..area.width).map(move |x| (x, y)))
      .filter_map(|(x, y)| buffer.cell((x, y)).map(|c| c.symbol().to_string()))
      .collect()
  }

  fn folder(name: &str, track_count: u32) -> PlaylistInfo {
    PlaylistInfo {
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
    }
  }

  #[test]
  fn song_table_fills_rows_below_scroll_padding() {
    let mut app = App::default();
    app.track_table.tracks = (0..30).map(track).collect();
    app.track_table.selected_index = 15;

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
    app.track_table.selected_index = 15;
    terminal.draw(|f| draw_song_table(f, &app, area)).unwrap();
    assert!(
      first_data_row(&terminal).contains("Track 9"),
      "view should scroll to offset 9"
    );

    // Moving the cursor back up within the window must NOT move the view.
    app.track_table.selected_index = 12;
    terminal.draw(|f| draw_song_table(f, &app, area)).unwrap();
    assert!(
      first_data_row(&terminal).contains("Track 9"),
      "view should stay anchored while the cursor moves inside the window"
    );

    // At the top visible row the view still holds...
    app.track_table.selected_index = 9;
    terminal.draw(|f| draw_song_table(f, &app, area)).unwrap();
    assert!(
      first_data_row(&terminal).contains("Track 9"),
      "cursor on the top visible row should not scroll yet"
    );

    // ...and only going past it scrolls the view up.
    app.track_table.selected_index = 8;
    terminal.draw(|f| draw_song_table(f, &app, area)).unwrap();
    assert!(
      first_data_row(&terminal).contains("Track 8"),
      "going above the top visible row should scroll the view up"
    );
  }

  #[test]
  fn local_browser_lists_folders_with_track_counts() {
    let mut app = App::default();
    app.local_playlists = vec![folder("MyAlbum", 3)];
    let content = rendered(&app, Rect::new(0, 0, 60, 6));
    assert!(
      content.contains("MyAlbum (3 tracks)"),
      "folder name and track count should render: {content}"
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
}
