use crate::core::app::{
  ActiveBlock, AlbumTableContext, App, EpisodeTableContext, RecommendationsContext,
};
use ratatui::{
  layout::{Constraint, Rect},
  style::{Modifier, Style},
  text::Span,
  widgets::{Block, Borders, Row, Table},
  Frame,
};
use rspotify::model::show::ResumePoint;
use rspotify::model::PlayableItem;
use rspotify::prelude::Id;

use super::util::{create_artist_string, get_color, get_percentage_width, millis_to_minutes};

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

#[derive(Default, PartialEq)]
pub enum ColumnId {
  #[default]
  None,
  Title,
  Liked,
}

pub struct TableHeader<'a> {
  pub id: TableId,
  pub items: Vec<TableHeaderItem<'a>>,
}

impl TableHeader<'_> {
  pub fn get_index(&self, id: ColumnId) -> Option<usize> {
    self.items.iter().position(|item| item.id == id)
  }
}

#[derive(Default)]
pub struct TableHeaderItem<'a> {
  pub id: ColumnId,
  pub text: &'a str,
  pub width: u16,
}

pub struct TableItem {
  pub id: String,
  pub format: Vec<String>,
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

  if let Some(saved_artists) = app.library.saved_artists.get_results(None) {
    let items = saved_artists
      .items
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
  } else {
    draw_table(
      f,
      app,
      layout_chunk,
      ("Artists", &header),
      &[],
      app.artists_list_index,
      highlight_state,
    )
  }
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
      .map(|show_page| {
        #[allow(deprecated)]
        let publisher = show_page.show.publisher.to_owned();
        TableItem {
          id: show_page.show.id.id().to_string(),
          format: vec![show_page.show.name.to_owned(), publisher],
        }
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
    highlight_state,
  )
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

    #[allow(deprecated)]
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

fn draw_table(
  f: &mut Frame<'_>,
  app: &App,
  layout_chunk: Rect,
  table_layout: (&str, &TableHeader), // (title, header colums)
  items: &[TableItem], // The nested vector must have the same length as the `header_columns`
  selected_index: usize,
  highlight_state: (bool, bool),
) {
  let selected_style = get_color(highlight_state, app.user_config.theme)
    .add_modifier(Modifier::BOLD | Modifier::REVERSED);

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
      _ => None,
    })
  });

  let (title, header) = table_layout;

  // Make sure that the selected item is visible on the page. Need to add some rows of padding
  // to chunk height for header and header space to get a true table height
  let padding = 5;
  let visible_rows = layout_chunk
    .height
    .checked_sub(padding)
    .map(|height| height as usize)
    .unwrap_or(0);

  let offset = table_scroll_offset(selected_index, visible_rows);

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

pub fn table_scroll_offset(selected_index: usize, visible_rows: usize) -> usize {
  if visible_rows == 0 {
    return 0;
  }

  selected_index.saturating_sub(visible_rows.saturating_sub(1))
}
