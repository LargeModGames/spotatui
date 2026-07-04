use crate::core::app::{App, ArtistBlock};
use ratatui::{
  layout::{Constraint, Layout, Rect},
  Frame,
};
use rspotify::model::PlayableItem;
use rspotify::prelude::Id;

use super::util::{draw_selectable_list, get_artist_highlight_state, join_artist_names};

pub fn draw_artist_albums(f: &mut Frame<'_>, app: &App, layout_chunk: Rect) {
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
  };
}
