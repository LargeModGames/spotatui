//! The now playing panel beside the main content: the cover and the details of
//! the playing item. Enter on the playbar opens and closes it; the layout
//! leaves it out while searching and when the content would get too narrow.

use crate::core::app::{App, PlayingTrackDetails};
use crate::core::format::millis_to_minutes;
use crate::core::plugin_api::PlayableInfo;
use crate::infra::media_metadata::{current_playback_snapshot, PlaybackSnapshot};
use crate::tui::theme::EmphasisExt;
use ratatui::{
  layout::{Alignment, Rect},
  style::{Modifier, Style},
  text::{Line, Span},
  widgets::{Block, BorderType, Borders, Paragraph},
  Frame,
};

/// How many queued items the panel lists.
const UP_NEXT_LEN: usize = 3;
/// The text rows the panel keeps below the cover, when it has them.
const MIN_TEXT_ROWS: u16 = 12;

/// What the panel shows, read from the app in one place so the draw is a plain
/// function of these values.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct NowPlayingView {
  pub title: String,
  pub artists: String,
  pub album: String,
  pub year: Option<String>,
  pub track_number: Option<String>,
  pub length: Option<String>,
  pub device: Option<String>,
  pub liked: bool,
  pub explicit: bool,
  pub up_next: Vec<String>,
}

impl NowPlayingView {
  /// The panel's values for the playing item, or `None` when nothing plays.
  pub(crate) fn from_app(app: &App) -> Option<Self> {
    let snapshot = current_playback_snapshot(app)?;
    Some(Self::build(
      &snapshot,
      app.playing_track_details(),
      |id| app.liked_song_ids_set().contains(id),
      queued_after(app, snapshot.item_uri.as_deref()),
    ))
  }

  /// The pure half of [`Self::from_app`].
  fn build(
    snapshot: &PlaybackSnapshot,
    details: Option<PlayingTrackDetails>,
    is_liked: impl Fn(&str) -> bool,
    up_next: Vec<String>,
  ) -> Self {
    // The cached context lags the native player after a skip: its details
    // count only while they name the item the snapshot names.
    let details = details.filter(|details| {
      details.id.is_some() && details.id.as_deref() == snapshot.item_id.as_deref()
    });
    let metadata = &snapshot.metadata;
    let track_number = details
      .as_ref()
      .filter(|details| details.track_number > 0)
      .map(|details| {
        if details.disc_number > 1 {
          format!("{} · disc {}", details.track_number, details.disc_number)
        } else {
          details.track_number.to_string()
        }
      });
    Self {
      title: metadata.title.clone(),
      artists: snapshot.primary_artist(),
      album: metadata.album.clone(),
      year: details.as_ref().and_then(|details| {
        super::util::release_year(details.release_date.as_deref()).map(str::to_string)
      }),
      track_number,
      length: (!snapshot.is_live && metadata.duration_ms > 0)
        .then(|| millis_to_minutes(u128::from(metadata.duration_ms))),
      device: details.as_ref().and_then(|details| details.device.clone()),
      liked: snapshot.item_id.as_deref().is_some_and(is_liked),
      explicit: details.as_ref().is_some_and(|details| details.explicit),
      up_next,
    }
  }
}

/// The names queued after the playing item, from the last queue fetch. A fetch
/// made for an earlier track says nothing about this one, so it counts only
/// while its playing item is the one playing now.
fn queued_after(app: &App, playing_uri: Option<&str>) -> Vec<String> {
  let Some(queue) = app.queue.as_ref() else {
    return Vec::new();
  };
  let fetched_for = queue.currently_playing.as_ref().and_then(playable_uri);
  if playing_uri.is_none() || fetched_for != playing_uri {
    return Vec::new();
  }
  queue
    .queue
    .iter()
    .take(UP_NEXT_LEN)
    .map(|item| match item {
      PlayableInfo::Track(track) => track.name.clone(),
      PlayableInfo::Episode(episode) => episode.name.clone(),
    })
    .collect()
}

fn playable_uri(item: &PlayableInfo) -> Option<&str> {
  match item {
    PlayableInfo::Track(track) => track.uri.as_deref(),
    PlayableInfo::Episode(episode) => episode.uri.as_deref(),
  }
}

pub fn draw_now_playing(f: &mut Frame<'_>, app: &App, area: Rect) {
  let theme = &app.user_config.theme;
  let strings = &app.strings().screens;
  let block = Block::default()
    .borders(Borders::ALL)
    .border_type(BorderType::Rounded)
    .title(Span::styled(
      strings.now_playing_title,
      Style::default().fg(theme.header.into()),
    ))
    .border_style(Style::default().fg(theme.inactive.into()));
  let inner = block.inner(area);
  f.render_widget(block, area);
  if inner.width == 0 || inner.height == 0 {
    return;
  }

  let Some(view) = NowPlayingView::from_app(app) else {
    let message = Paragraph::new(strings.now_playing_nothing)
      .style(Style::default().fg(theme.inactive.into()))
      .alignment(Alignment::Center);
    f.render_widget(
      message,
      Rect {
        y: inner.y + inner.height / 2,
        height: 1,
        ..inner
      },
    );
    return;
  };

  let lines = detail_lines(app, &view, inner.width);
  let text_rows = (lines.len() as u16).min(inner.height);
  let art_area = cover_area(inner, text_rows);
  let art_rows = draw_cover(f, app, art_area);

  let text_y = inner.y + art_rows;
  let text_area = Rect {
    y: text_y,
    height: (inner.y + inner.height).saturating_sub(text_y),
    ..inner
  };
  f.render_widget(Paragraph::new(lines), text_area);
}

/// The rect above the text the cover may take: square-ish (a cell is about
/// twice as tall as it is wide), never pushing the text below `MIN_TEXT_ROWS`.
fn cover_area(inner: Rect, text_rows: u16) -> Rect {
  let reserved = text_rows
    .min(MIN_TEXT_ROWS)
    .max(text_rows.min(inner.height / 2));
  let height = (inner.width / 2).min(inner.height.saturating_sub(reserved));
  Rect { height, ..inner }
}

/// Draw the cover centered in `area` and return how many rows it took,
/// including the blank row under it.
#[cfg(feature = "cover-art")]
fn draw_cover(f: &mut Frame<'_>, app: &App, area: Rect) -> u16 {
  use crate::tui::theme::ThemeExt;
  use ratatui::widgets::Clear;
  if area.height < 3 || area.width < 6 {
    return 0;
  }
  let theme = &app.user_config.theme;
  let padded = Rect {
    x: area.x + 1,
    y: area.y + 1,
    width: area.width.saturating_sub(2),
    height: area.height.saturating_sub(2),
  };
  // Clear first: an image protocol leaves its pixels behind when the art changes size.
  // Clear also resets the cells to the terminal default, so paint the theme
  // background back before anything is drawn over them.
  f.render_widget(Clear, area);
  f.render_widget(Block::default().style(theme.base_style()), area);
  // Same gate as the playbar: the art can be decoded for the adaptive theme
  // while the user turned the picture off, or the terminal cannot show it.
  if !app
    .user_config
    .do_draw_cover_art(crate::tui::cover_art::full_image_support())
  {
    return 0;
  }
  if !app.cover_art.available() {
    let message = Paragraph::new(super::util::cover_art_status(app))
      .style(Style::default().fg(theme.inactive.into()))
      .alignment(Alignment::Center);
    f.render_widget(
      message,
      Rect {
        y: area.y + area.height / 2,
        height: 1,
        ..area
      },
    );
    return area.height;
  }
  let bounds = Rect::new(0, 0, padded.width, padded.height);
  let fitted =
    crate::tui::cover_art::now_playing_size_for(bounds, &app.cover_art).unwrap_or(bounds);
  let centered = crate::tui::layout::center_rect_within(padded, fitted);
  crate::tui::cover_art::render_now_playing(f, centered, &app.cover_art, &app.user_config);
  (centered.y + centered.height + 1).saturating_sub(area.y)
}

/// Without the cover-art feature the panel is text only.
#[cfg(not(feature = "cover-art"))]
fn draw_cover(_f: &mut Frame<'_>, _app: &App, _area: Rect) -> u16 {
  0
}

fn detail_lines(app: &App, view: &NowPlayingView, width: u16) -> Vec<Line<'static>> {
  let theme = &app.user_config.theme;
  let strings = &app.strings().screens;
  let behavior = &app.user_config.behavior;
  let text_width = usize::from(width.saturating_sub(2));
  let fit = |text: &str| super::util::truncate_text(text, text_width);
  let label_style = Style::default().fg(theme.inactive.into());
  let value_style = Style::default().fg(theme.text.into());

  let mut lines = vec![
    Line::from(Span::styled(
      format!(" {}", fit(&view.title)),
      Style::default()
        .fg(theme.text.into())
        .add_modifier(behavior.emphasis(Modifier::BOLD)),
    )),
    Line::from(Span::styled(
      format!(" {}", fit(&view.artists)),
      Style::default().fg(theme.active.into()),
    )),
    Line::default(),
  ];

  let labels = [
    strings.now_playing_album,
    strings.now_playing_year,
    strings.now_playing_track,
    strings.now_playing_length,
    strings.now_playing_device,
  ];
  let label_width = labels
    .iter()
    .map(|label| unicode_width::UnicodeWidthStr::width(*label))
    .max()
    .unwrap_or(0)
    + 2;
  let value_width = text_width.saturating_sub(label_width);
  let rows = [
    (
      strings.now_playing_album,
      Some(view.album.clone()).filter(|album| !album.is_empty()),
    ),
    (strings.now_playing_year, view.year.clone()),
    (strings.now_playing_track, view.track_number.clone()),
    (strings.now_playing_length, view.length.clone()),
    (strings.now_playing_device, view.device.clone()),
  ];
  for (label, value) in rows {
    let Some(value) = value else { continue };
    lines.push(Line::from(vec![
      Span::styled(
        format!(" {}", super::util::pad_to_width(label, label_width)),
        label_style,
      ),
      Span::styled(super::util::truncate_text(&value, value_width), value_style),
    ]));
  }

  let mut badges = Vec::new();
  if view.liked {
    badges.push(Span::styled(
      format!(
        " {}{}",
        app.user_config.padded_liked_icon(),
        strings.now_playing_liked
      ),
      Style::default().fg(theme.active.into()),
    ));
  }
  if view.explicit {
    badges.push(Span::styled(
      format!(" [{}]", strings.now_playing_explicit),
      label_style,
    ));
  }
  if !badges.is_empty() {
    lines.push(Line::from(badges));
  }

  if !view.up_next.is_empty() {
    lines.push(Line::default());
    let heading = format!(" {} ", strings.now_playing_up_next);
    let rule = "─"
      .repeat(text_width.saturating_sub(unicode_width::UnicodeWidthStr::width(heading.as_str())));
    lines.push(Line::from(Span::styled(
      format!("{heading}{rule}"),
      label_style,
    )));
    for (index, name) in view.up_next.iter().enumerate() {
      let prefix = format!(" {}. ", index + 1);
      let room = text_width.saturating_sub(prefix.len());
      lines.push(Line::from(vec![
        Span::styled(prefix, label_style),
        Span::styled(super::util::truncate_text(name, room), value_style),
      ]));
    }
  }
  lines
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::infra::media_metadata::{PlaybackItemKind, PlaybackMetadata, PlaybackSource};

  fn snapshot(item_id: Option<&str>, duration_ms: u32, is_live: bool) -> PlaybackSnapshot {
    PlaybackSnapshot {
      metadata: PlaybackMetadata {
        title: "Fire in These Hills".to_string(),
        artists: vec!["Imagine Dragons".to_string()],
        album: "LOOM".to_string(),
        image_url: None,
        duration_ms,
      },
      item_kind: PlaybackItemKind::Track,
      item_id: item_id.map(str::to_string),
      item_uri: item_id.map(|id| format!("spotify:track:{id}")),
      context_uri: None,
      source: PlaybackSource::NativeContext,
      progress_ms: 0,
      is_playing: true,
      is_live,
      shuffle: false,
      repeat: None,
    }
  }

  fn details(id: &str) -> PlayingTrackDetails {
    PlayingTrackDetails {
      id: Some(id.to_string()),
      release_date: Some("2024-06-28".to_string()),
      track_number: 3,
      disc_number: 1,
      explicit: true,
      device: Some("spotatui".to_string()),
    }
  }

  #[test]
  fn details_of_the_playing_track_fill_year_track_and_device() {
    let view = NowPlayingView::build(
      &snapshot(Some("abc"), 219_000, false),
      Some(details("abc")),
      |id| id == "abc",
      vec!["Thunder".to_string()],
    );

    assert_eq!(view.title, "Fire in These Hills");
    assert_eq!(view.artists, "Imagine Dragons");
    assert_eq!(view.year.as_deref(), Some("2024"));
    assert_eq!(view.track_number.as_deref(), Some("3"));
    assert_eq!(view.length.as_deref(), Some("3:39"));
    assert_eq!(view.device.as_deref(), Some("spotatui"));
    assert!(view.liked);
    assert!(view.explicit);
    assert_eq!(view.up_next, vec!["Thunder".to_string()]);
  }

  #[test]
  fn details_of_a_previous_track_are_ignored_after_a_skip() {
    let view = NowPlayingView::build(
      &snapshot(Some("new"), 219_000, false),
      Some(details("old")),
      |_| false,
      Vec::new(),
    );

    assert_eq!(view.year, None);
    assert_eq!(view.track_number, None);
    assert_eq!(view.device, None);
    assert!(!view.explicit);
    assert_eq!(view.title, "Fire in These Hills");
  }

  #[test]
  fn a_second_disc_is_named_beside_the_track_number() {
    let mut on_disc_two = details("abc");
    on_disc_two.disc_number = 2;
    let view = NowPlayingView::build(
      &snapshot(Some("abc"), 1, false),
      Some(on_disc_two),
      |_| false,
      Vec::new(),
    );

    assert_eq!(view.track_number.as_deref(), Some("3 · disc 2"));
  }

  #[test]
  fn a_live_stream_has_no_length() {
    let view = NowPlayingView::build(&snapshot(None, 0, true), None, |_| true, Vec::new());

    assert_eq!(view.length, None);
    assert!(!view.liked, "an item with no id is never liked");
  }

  #[test]
  fn the_cover_leaves_the_text_its_rows() {
    let inner = Rect::new(0, 0, 38, 40);

    let area = cover_area(inner, 14);

    assert_eq!(area.height, 19, "square-ish cover at half the width");
    let short = cover_area(Rect::new(0, 0, 38, 20), 14);
    assert_eq!(
      short.height, 8,
      "a short panel shrinks the cover, not the text"
    );
  }

  #[cfg(feature = "cover-art")]
  #[test]
  fn the_panel_keeps_no_rows_for_the_cover_when_cover_art_is_turned_off() {
    use ratatui::{backend::TestBackend, Terminal};
    let mut app = App::default();
    app.user_config.behavior.draw_cover_art = false;
    let area = Rect::new(0, 0, 38, 19);
    let mut terminal = Terminal::new(TestBackend::new(40, 20)).unwrap();

    let mut rows = None;
    terminal
      .draw(|f| rows = Some(draw_cover(f, &app, area)))
      .unwrap();

    assert_eq!(rows, Some(0), "the text starts at the top of the panel");
  }

  #[cfg(feature = "cover-art")]
  #[test]
  fn the_cleared_cover_area_keeps_the_theme_background() {
    use crate::core::theme::Color;
    use ratatui::{backend::TestBackend, Terminal};
    let mut app = App::default();
    app.user_config.theme.background = Color::Rgb(250, 250, 240);
    let area = Rect::new(0, 0, 38, 19);
    let mut terminal = Terminal::new(TestBackend::new(40, 20)).unwrap();

    terminal
      .draw(|f| {
        draw_cover(f, &app, area);
      })
      .unwrap();

    let cell = terminal.backend().buffer().cell((5, 2)).unwrap().clone();
    assert_eq!(
      cell.bg,
      app.user_config.theme.background.into(),
      "Clear must not leave the terminal default behind"
    );
  }
}
