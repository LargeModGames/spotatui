use crate::core::app::{App, ArtistBlock};
use ratatui::{
  layout::{Constraint, Layout, Rect},
  Frame,
};
use rspotify::model::PlayableItem;
use rspotify::prelude::Id;

use super::util::{draw_selectable_list, get_artist_highlight_state, join_artist_names};

pub fn draw_artist_albums(f: &mut Frame<'_>, app: &App, layout_chunk: Rect) {
  if let Some(artist) = &app.artist {
    let (tracks_area, albums_area, related_artists_area) = if artist.related_artists_visible() {
      let [tracks, albums, related] = layout_chunk.layout(&Layout::horizontal([
        Constraint::Percentage(33),
        Constraint::Percentage(33),
        Constraint::Percentage(33),
      ]));
      (tracks, albums, Some(related))
    } else {
      let [tracks, albums] = layout_chunk.layout(&Layout::horizontal([
        Constraint::Percentage(50),
        Constraint::Percentage(50),
      ]));
      (tracks, albums, None)
    };

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

          if track_id.is_some() && track_id == top_track.id {
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
      &format!("{} - Top Tracks", artist.artist_name),
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
          if app.saved_album_ids_set.contains(album_id.as_str()) {
            album_artist.push_str(&app.user_config.padded_liked_icon());
          }
        }
        album_artist.push_str(&format!(
          "{} - {} ({})",
          item.name.to_owned(),
          join_artist_names(&item.artists),
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

    if let Some(related_artists_area) = related_artists_area {
      let related_artists = artist
        .related_artists
        .iter()
        .map(|item| {
          let mut artist = String::new();
          if let Some(artist_id) = &item.id {
            if app.followed_artist_ids_set.contains(artist_id.as_str()) {
              artist.push_str(&app.user_config.padded_liked_icon());
            }
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
    }
  };
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::app::Artist;
  use crate::core::pagination::Paged;
  use crate::core::plugin_api::{AlbumInfo, ArtistInfo, TrackInfo};
  use ratatui::{backend::TestBackend, Terminal};

  fn top_track(name: &str) -> TrackInfo {
    TrackInfo {
      uri: Some("spotify:track:0000000000000000000001".to_string()),
      name: name.to_string(),
      artists: vec!["First".to_string()],
      album: "Album".to_string(),
      duration_ms: 180_000,
      id: Some("0000000000000000000001".to_string()),
      album_id: None,
      artist_refs: Vec::new(),
      is_playable: true,
      is_local: false,
      track_number: 1,
      explicit: false,
      image_url: None,
    }
  }

  fn artist_page(related_artists: Vec<ArtistInfo>) -> Artist {
    Artist {
      artist_id: "artist1".to_string(),
      artist_name: "First".to_string(),
      albums: Paged {
        items: vec![AlbumInfo {
          id: Some("album1".to_string()),
          uri: Some("spotify:album:album1".to_string()),
          name: "Album".to_string(),
          album_type: Some("album".to_string()),
          ..Default::default()
        }],
        ..Default::default()
      },
      related_artists,
      top_tracks: vec![top_track("One")],
      selected_album_index: 0,
      selected_related_artist_index: 0,
      selected_top_track_index: 0,
      artist_selected_block: ArtistBlock::TopTracks,
      artist_hovered_block: ArtistBlock::TopTracks,
    }
  }

  #[test]
  fn empty_related_artists_renders_two_equal_columns_without_the_third_title() {
    let mut app = App::default();
    app.artist = Some(artist_page(Vec::new()));

    let area = Rect::new(0, 0, 90, 20);
    let mut terminal = Terminal::new(TestBackend::new(area.width, area.height)).unwrap();
    terminal
      .draw(|f| draw_artist_albums(f, &app, area))
      .unwrap();

    let buffer = terminal.backend().buffer();
    let content: String = (0..area.height)
      .flat_map(|y| (0..area.width).map(move |x| (x, y)))
      .filter_map(|(x, y)| buffer.cell((x, y)).map(|c| c.symbol().to_string()))
      .collect();
    assert!(
      content.contains("Top Tracks"),
      "top tracks column should render: {content}"
    );
    assert!(
      content.contains("Albums"),
      "albums column should render: {content}"
    );
    assert!(
      !content.contains("Related artists"),
      "no third column should render for an empty list: {content}"
    );

    // Compare by cell, not by byte: the rounded-border glyphs are multi-byte,
    // so `str::find` would report a byte offset rather than the column.
    let row: Vec<String> = (0..area.width)
      .filter_map(|x| buffer.cell((x, 0)).map(|c| c.symbol().to_string()))
      .collect();
    let albums_x = row
      .windows("Albums".len())
      .position(|window| window.join("") == "Albums")
      .expect("albums title should sit on the top border row");
    assert_eq!(
      albums_x,
      46,
      "albums panel should start at the halfway mark (50/50 split): {}",
      row.join("")
    );
  }
}
