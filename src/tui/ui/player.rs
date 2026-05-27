use crate::core::{
  app::{ActiveBlock, App},
  layout::{fullscreen_view_layout, miniplayer_playbar_area},
};
use ratatui::{
  layout::{Alignment, Constraint, Layout, Position, Rect},
  style::{Color, Modifier, Style},
  text::{Line, Span, Text},
  widgets::{
    canvas::Canvas, Block, BorderType, Borders, LineGauge, List, ListItem, ListState, Paragraph,
    Wrap,
  },
  Frame,
};
use rspotify::model::enums::RepeatState;
use rspotify::model::PlayableItem;
use rspotify::prelude::Id;

use super::util::{
  create_artist_string, display_track_progress, get_color, get_track_progress_percentage,
};

const PLAYBAR_CONTROLS: [PlaybarControl; 8] = [
  PlaybarControl::Prev,
  PlaybarControl::PlayPause,
  PlaybarControl::Next,
  PlaybarControl::Shuffle,
  PlaybarControl::Repeat,
  PlaybarControl::Like,
  PlaybarControl::VolumeDown,
  PlaybarControl::VolumeUp,
];
#[cfg(feature = "cover-art")]
const COVER_ART_CELL_RATIO: f32 = 1.9;
#[cfg(feature = "cover-art")]
const PLAYBAR_TRACK_INFO_ROWS: u16 = 2;
#[cfg(feature = "cover-art")]
const PLAYBAR_PROGRESS_ROWS: u16 = 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PlaybarControl {
  Prev,
  PlayPause,
  Next,
  Shuffle,
  Repeat,
  Like,
  VolumeDown,
  VolumeUp,
}

impl PlaybarControl {
  const fn button_label(self) -> &'static str {
    match self {
      Self::Prev => "[Prev]",
      Self::PlayPause => "[Play/Pause]",
      Self::Next => "[Next]",
      Self::Shuffle => "[Shuffle]",
      Self::Repeat => "[Repeat]",
      Self::Like => "[Like]",
      Self::VolumeDown => "[Vol-]",
      Self::VolumeUp => "[Vol+]",
    }
  }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PlaybarControlHitbox {
  control: PlaybarControl,
  rect: Rect,
}

#[derive(Clone, Copy, Debug)]
struct PlaybarLayoutAreas {
  artist_area: Rect,
  controls_area: Rect,
  progress_area: Rect,
  #[cfg(feature = "cover-art")]
  cover_art: Option<Rect>,
}

#[cfg(feature = "cover-art")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PlaybarCoverLayout {
  text_area: Rect,
  slot: Rect,
  image_area: Rect,
}

fn split_playbar_rows(area: Rect) -> (Rect, Rect, Rect) {
  if area.width == 0 || area.height == 0 {
    let empty = Rect::new(area.x, area.y, area.width, 0);
    return (empty, empty, empty);
  }

  if area.height == 1 {
    let empty = Rect::new(area.x, area.y, area.width, 0);
    return (empty, area, empty);
  }

  if area.height == 2 {
    let [controls_area, progress_area] = area.layout(&Layout::vertical([
      Constraint::Length(1),
      Constraint::Length(1),
    ]));
    let empty = Rect::new(area.x, area.y, area.width, 0);
    return (empty, controls_area, progress_area);
  }

  let [artist_area, controls_area, progress_area] = area.layout(&Layout::vertical([
    Constraint::Min(1),
    Constraint::Length(1),
    Constraint::Length(1),
  ]));

  (artist_area, controls_area, progress_area)
}

fn playbar_layout_areas(app: &App, layout_chunk: Rect) -> PlaybarLayoutAreas {
  #[cfg(feature = "cover-art")]
  {
    // first create margins
    let [other] = layout_chunk.layout(&Layout::horizontal([Constraint::Fill(1)]).margin(1));

    let (other, cover_art) = if app
      .user_config
      .do_draw_cover_art(app.cover_art.full_image_support())
    {
      let cover_layout = playbar_cover_layout(
        other,
        app.user_config.behavior.playbar_cover_art_size_percent,
      );
      if let Some(rendered_size) = app.cover_art.size_for(cover_layout.image_area) {
        let cover_layout = cover_layout.with_rendered_size(rendered_size);
        let (artist_area, controls_area, progress_area) =
          split_cover_playbar_rows(other, cover_layout.text_area, cover_layout.image_area);

        return PlaybarLayoutAreas {
          artist_area,
          controls_area,
          progress_area,
          cover_art: Some(cover_layout.image_area),
        };
      } else {
        (other, None)
      }
    } else {
      (other, None)
    };

    let (artist_area, controls_area, progress_area) = split_playbar_rows(other);

    PlaybarLayoutAreas {
      artist_area,
      controls_area,
      progress_area,
      cover_art,
    }
  }

  #[cfg(not(feature = "cover-art"))]
  {
    let _ = app;
    let [inner] = layout_chunk.layout(&Layout::horizontal([Constraint::Fill(1)]).margin(1));
    let (artist_area, controls_area, progress_area) = split_playbar_rows(inner);

    PlaybarLayoutAreas {
      artist_area,
      controls_area,
      progress_area,
    }
  }
}

#[cfg(feature = "cover-art")]
fn playbar_cover_layout(inner: Rect, size_percent: u16) -> PlaybarCoverLayout {
  let size_percent = crate::core::user_config::clamp_playbar_cover_art_size_percent(size_percent);
  let image_height = scaled_cover_art_height(inner.height, size_percent);
  let requested_width = ((image_height as f32) * COVER_ART_CELL_RATIO).ceil() as u16;
  let max_slot_width = if inner.width > 2 {
    inner.width.saturating_sub(2)
  } else {
    inner.width
  };
  let slot_width = requested_width.min(max_slot_width);
  let separator_width = u16::from(slot_width > 0 && inner.width > slot_width);

  let slot = Rect::new(inner.x, inner.y, slot_width, inner.height);
  let text_x = inner
    .x
    .saturating_add(slot_width.saturating_add(separator_width));
  let text_width = inner
    .width
    .saturating_sub(slot_width.saturating_add(separator_width));
  let text_area = Rect::new(text_x, inner.y, text_width, inner.height);
  let image_area = center_rect_within(
    slot,
    Rect::new(0, 0, requested_width.min(slot_width), image_height),
  );

  PlaybarCoverLayout {
    text_area,
    slot,
    image_area,
  }
}

#[cfg(feature = "cover-art")]
fn scaled_cover_art_height(available_height: u16, size_percent: u16) -> u16 {
  if available_height == 0 {
    return 0;
  }

  let size_percent = crate::core::user_config::clamp_playbar_cover_art_size_percent(size_percent);
  let target_percent = if size_percent <= 100 {
    25 + ((size_percent.saturating_sub(25) as u32 * 35).saturating_add(74) / 75) as u16
  } else {
    60 + (((size_percent - 100) as u32 * 40).saturating_add(99) / 100) as u16
  };

  (((available_height as u32 * target_percent as u32).saturating_add(99) / 100) as u16)
    .clamp(1, available_height)
}

#[cfg(feature = "cover-art")]
impl PlaybarCoverLayout {
  fn with_rendered_size(self, rendered_size: Rect) -> Self {
    Self {
      image_area: bottom_aligned_rect_within(self.image_area, rendered_size),
      ..self
    }
  }
}

#[cfg(feature = "cover-art")]
fn bottom_aligned_rect_within(bounds: Rect, size: Rect) -> Rect {
  let width = size.width.min(bounds.width);
  let height = size.height.min(bounds.height);

  Rect {
    x: bounds.x + bounds.width.saturating_sub(width) / 2,
    y: bounds.y + bounds.height.saturating_sub(height),
    width,
    height,
  }
}

#[cfg(feature = "cover-art")]
fn split_cover_playbar_rows(inner: Rect, text_area: Rect, image_area: Rect) -> (Rect, Rect, Rect) {
  if inner.width == 0 || inner.height == 0 || text_area.width == 0 || text_area.height == 0 {
    let empty = Rect::new(text_area.x, text_area.y, text_area.width, 0);
    return (empty, empty, empty);
  }

  let progress_y = inner
    .y
    .saturating_add(inner.height.saturating_sub(PLAYBAR_PROGRESS_ROWS));
  let image_bottom = image_area.y.saturating_add(image_area.height);
  let progress_area = if image_bottom <= progress_y {
    Rect::new(inner.x, progress_y, inner.width, PLAYBAR_PROGRESS_ROWS)
  } else {
    Rect::new(
      text_area.x,
      progress_y,
      text_area.width,
      PLAYBAR_PROGRESS_ROWS,
    )
  };

  let controls_area = cover_playbar_controls_area(inner, text_area, image_area, progress_area);
  let artist_area = cover_playbar_artist_area(text_area, image_area, controls_area, progress_area);

  (artist_area, controls_area, progress_area)
}

#[cfg(feature = "cover-art")]
fn cover_playbar_controls_area(
  inner: Rect,
  text_area: Rect,
  image_area: Rect,
  progress_area: Rect,
) -> Rect {
  let required_width = playbar_controls_required_width();
  let controls_y = progress_area.y.saturating_sub(1);
  let image_bottom = image_area.y.saturating_add(image_area.height);

  if image_bottom <= controls_y && inner.width >= required_width {
    return Rect::new(inner.x, controls_y, inner.width, 1);
  }

  let artist_y = image_area.y.max(text_area.y);
  let available_text_rows = controls_y.saturating_sub(artist_y);
  if text_area.width >= required_width && available_text_rows >= PLAYBAR_TRACK_INFO_ROWS {
    Rect::new(text_area.x, controls_y, text_area.width, 1)
  } else {
    Rect::new(text_area.x, text_area.y, text_area.width, 0)
  }
}

#[cfg(feature = "cover-art")]
fn cover_playbar_artist_area(
  text_area: Rect,
  image_area: Rect,
  controls_area: Rect,
  progress_area: Rect,
) -> Rect {
  let y = image_area.y.max(text_area.y);
  let bottom = if controls_area.height > 0 {
    controls_area.y
  } else {
    progress_area.y
  };
  let height = bottom.saturating_sub(y).min(PLAYBAR_TRACK_INFO_ROWS);

  Rect::new(text_area.x, y, text_area.width, height)
}

fn playbar_control_hitboxes_in_area(controls_area: Rect) -> Vec<PlaybarControlHitbox> {
  if controls_area.width == 0 || controls_area.height == 0 {
    return Vec::new();
  }

  let required_width = playbar_controls_required_width();
  let start_x = if controls_area.width > required_width {
    controls_area
      .x
      .saturating_add((controls_area.width - required_width) / 2)
  } else {
    controls_area.x
  };

  let mut x = start_x;
  let y = controls_area.y.saturating_add(controls_area.height / 2);
  let right = controls_area.x.saturating_add(controls_area.width);
  let mut hitboxes = Vec::with_capacity(PLAYBAR_CONTROLS.len());

  for control in PLAYBAR_CONTROLS {
    let width = control.button_label().len() as u16;
    if x.saturating_add(width) > right {
      break;
    }
    hitboxes.push(PlaybarControlHitbox {
      control,
      rect: Rect {
        x,
        y,
        width,
        height: 1,
      },
    });
    x = x.saturating_add(width.saturating_add(1));
  }

  hitboxes
}

fn playbar_controls_required_width() -> u16 {
  PLAYBAR_CONTROLS
    .iter()
    .enumerate()
    .fold(0u16, |width, (idx, control)| {
      width
        .saturating_add(u16::from(idx > 0))
        .saturating_add(control.button_label().len() as u16)
    })
}

pub(crate) fn playbar_control_hitboxes(
  app: &App,
  playbar_area: Rect,
) -> Vec<(PlaybarControl, Rect)> {
  if app
    .current_playback_context
    .as_ref()
    .and_then(|ctx| ctx.item.as_ref())
    .is_none()
  {
    return Vec::new();
  }

  let controls_area = playbar_layout_areas(app, playbar_area).controls_area;
  playbar_control_hitboxes_in_area(controls_area)
    .into_iter()
    .map(|hitbox| (hitbox.control, hitbox.rect))
    .collect()
}

pub(crate) fn playbar_control_at(
  app: &App,
  playbar_area: Rect,
  x: u16,
  y: u16,
) -> Option<PlaybarControl> {
  playbar_control_hitboxes(app, playbar_area)
    .into_iter()
    .find_map(|(control, rect)| rect.contains(Position { x, y }).then_some(control))
}

fn draw_playbar_controls(f: &mut Frame<'_>, app: &App, controls_area: Rect) {
  let controls_style = Style::default().fg(app.user_config.theme.playbar_text);
  for hitbox in playbar_control_hitboxes_in_area(controls_area) {
    let control = Paragraph::new(Span::styled(hitbox.control.button_label(), controls_style));
    f.render_widget(control, hitbox.rect);
  }
}

#[cfg(feature = "cover-art")]
fn center_rect_within(bounds: Rect, size: Rect) -> Rect {
  Rect {
    x: bounds.x + bounds.width.saturating_sub(size.width.min(bounds.width)) / 2,
    y: bounds.y + bounds.height.saturating_sub(size.height.min(bounds.height)) / 2,
    width: size.width.min(bounds.width),
    height: size.height.min(bounds.height),
  }
}

pub fn draw_lyrics_view(f: &mut Frame<'_>, app: &App) {
  let (content_area, playbar_area) = fullscreen_view_layout(&app.user_config.behavior, f.area());

  draw_lyrics(f, app, content_area);
  if let Some(playbar_area) = playbar_area {
    draw_playbar(f, app, playbar_area);
  }
}

#[cfg(feature = "cover-art")]
pub fn draw_cover_art_view(f: &mut Frame<'_>, app: &App) {
  let (content_area, playbar_area) = fullscreen_view_layout(&app.user_config.behavior, f.area());

  draw_cover_art_content(f, app, content_area);
  if let Some(playbar_area) = playbar_area {
    draw_playbar(f, app, playbar_area);
  }
}

pub fn draw_miniplayer(f: &mut Frame<'_>, app: &App) {
  let area = miniplayer_playbar_area(f.area());
  draw_playbar(f, app, area);
}

#[cfg(feature = "cover-art")]
fn draw_cover_art_content(f: &mut Frame<'_>, app: &App, area: Rect) {
  use ratatui::widgets::Clear;

  // Clear the area to remove any lingering terminal image protocol artifacts
  f.render_widget(Clear, area);

  // Extract track info for display below the cover art
  let (track_name, artist_str) = extract_track_info(app);

  if !app.cover_art.available() {
    let p = Paragraph::new("No cover art available")
      .style(Style::default().fg(Color::Rgb(100, 100, 100)))
      .alignment(Alignment::Center);

    let vertical_center = area.y + area.height / 2;
    let center_area = Rect {
      x: area.x,
      y: vertical_center,
      width: area.width,
      height: 1,
    };
    f.render_widget(p, center_area);
    return;
  }

  let show_title = track_name.is_some();
  let show_artist = show_title && artist_str.is_some();
  let info_height = if show_title {
    1 + 1 + u16::from(show_artist)
  } else {
    0
  };
  let image_bounds = Rect {
    x: area.x,
    y: area.y,
    width: area.width,
    height: area.height.saturating_sub(info_height),
  };
  let available_image_size = Rect::new(
    0,
    0,
    image_bounds.width.saturating_sub(2),
    image_bounds.height.saturating_sub(2),
  );
  let fitted_image_size = app
    .cover_art
    .fullscreen_size_for(available_image_size)
    .unwrap_or(available_image_size);
  let centered_area = center_rect_within(image_bounds, fitted_image_size);

  app.cover_art.render_fullscreen(f, centered_area);

  // Draw song info below the cover art
  if let Some(name) = track_name {
    let title_y = centered_area.y + centered_area.height + 1;
    if title_y < area.y + area.height {
      let title = Paragraph::new(name)
        .style(
          Style::default()
            .fg(app.user_config.theme.selected)
            .add_modifier(Modifier::BOLD),
        )
        .alignment(Alignment::Center);
      f.render_widget(
        title,
        Rect {
          x: area.x,
          y: title_y,
          width: area.width,
          height: 1,
        },
      );
    }

    if let Some(artists) = artist_str {
      let artist_y = title_y + 1;
      if artist_y < area.y + area.height {
        let artist = Paragraph::new(artists)
          .style(Style::default().fg(app.user_config.theme.playbar_text))
          .alignment(Alignment::Center);
        f.render_widget(
          artist,
          Rect {
            x: area.x,
            y: artist_y,
            width: area.width,
            height: 1,
          },
        );
      }
    }
  }
}

#[cfg(feature = "cover-art")]
fn extract_track_info(app: &App) -> (Option<String>, Option<String>) {
  use rspotify::model::PlayableItem;

  // Prefer native track info (more responsive after skipping tracks)
  if let Some(ref native_info) = app.native_track_info {
    return (
      Some(native_info.name.clone()),
      Some(native_info.artists_display.clone()),
    );
  }

  if let Some(ctx) = &app.current_playback_context {
    if let Some(track_item) = &ctx.item {
      let (name, artists) = match track_item {
        PlayableItem::Track(track) => (track.name.clone(), create_artist_string(&track.artists)),
        PlayableItem::Episode(episode) => (episode.name.clone(), episode.show.name.clone()),
        _ => return (None, None),
      };
      return (Some(name), Some(artists));
    }
  }

  (None, None)
}

fn draw_lyrics(f: &mut Frame<'_>, app: &App, area: Rect) {
  use crate::core::app::LyricsStatus;

  // Draw bordered block first
  let block = Block::default()
    .borders(Borders::ALL)
    .title(" Lyrics ")
    .style(Style::default().fg(Color::Rgb(100, 100, 100))); // RGB for cross-terminal compat
  f.render_widget(block.clone(), area);

  let inner_area = block.inner(area);

  if app.lyrics_status != LyricsStatus::Found {
    let text = match app.lyrics_status {
      LyricsStatus::Loading => "Loading lyrics...",
      LyricsStatus::NotFound => "No lyrics found for this track.",
      LyricsStatus::NotStarted => "Waiting for track update...",
      LyricsStatus::Found => "",
    };

    if !text.is_empty() {
      let p = Paragraph::new(text)
        .style(Style::default().fg(Color::Rgb(100, 100, 100))) // RGB for cross-terminal compat
        .alignment(Alignment::Center);

      // Center vertically in inner area
      let vertical_center = inner_area.y + inner_area.height / 2;
      let top_area = Rect {
        x: inner_area.x,
        y: vertical_center.saturating_sub(0), // Just one line centered
        width: inner_area.width,
        height: 1,
      };
      f.render_widget(p, top_area);
    }
    return;
  }

  if let Some(lyrics) = &app.lyrics {
    if lyrics.is_empty() {
      return;
    }

    let current_time = app.song_progress_ms;
    let mut active_idx = 0;
    for (i, (time, _)) in lyrics.iter().enumerate() {
      if *time <= current_time {
        active_idx = i;
      } else {
        break;
      }
    }

    // Target position for active line: Vertical center of inner_area
    let target_row = inner_area.y + (inner_area.height / 2);

    let area_height = inner_area.height as i32;
    let area_y = inner_area.y as i32;

    // Loop through all visible rows of the screen area
    for row in 0..area_height {
      let screen_y = area_y + row;

      // screen_y = target_row + (line_idx - active_idx)
      // line_idx = screen_y - target_row + active_idx

      let offset_from_target = screen_y - (target_row as i32);
      let line_idx = active_idx as i32 + offset_from_target;

      if line_idx >= 0 && line_idx < lyrics.len() as i32 {
        let (_, text) = &lyrics[line_idx as usize];
        let is_active = line_idx == active_idx as i32;

        // Use explicit RGB colors for cross-terminal compatibility
        // Some terminals (like Kitty with custom themes) remap ANSI colors
        let style = if is_active {
          Style::default()
            .fg(app.user_config.theme.highlighted_lyrics) // Use theme color for highlighted lyrics
            .add_modifier(Modifier::BOLD)
        } else {
          Style::default().fg(Color::Rgb(100, 100, 100)) // Dim gray for inactive lines
        };

        let p = Paragraph::new(text.clone())
          .style(style)
          .alignment(Alignment::Center);

        let line_rect = Rect {
          x: inner_area.x,
          y: screen_y as u16,
          width: inner_area.width,
          height: 1,
        };
        f.render_widget(p, line_rect);
      }
    }
  }
}

pub fn draw_playbar(f: &mut Frame<'_>, app: &App, layout_chunk: Rect) {
  let playbar_areas = playbar_layout_areas(app, layout_chunk);
  let artist_area = playbar_areas.artist_area;
  let progress_area = playbar_areas.progress_area;

  let mut drew_playbar = false;

  // If no track is playing, render paragraph showing which device is selected, if no selected
  // give hint to choose a device
  if let Some(current_playback_context) = &app.current_playback_context {
    if let Some(track_item) = &current_playback_context.item {
      // Use native playing state when streaming is active (more reliable for MPRIS controls)
      let is_playing = app
        .native_is_playing
        .filter(|_| app.is_streaming_active)
        .unwrap_or(current_playback_context.is_playing);

      let play_title = if is_playing { "Playing" } else { "Paused" };

      let shuffle_text = if current_playback_context.shuffle_state {
        "On"
      } else {
        "Off"
      };

      let repeat_text = match current_playback_context.repeat_state {
        RepeatState::Off => "Off",
        RepeatState::Track => "Track",
        RepeatState::Context => "All",
      };

      let mut title = format!(
        "{:-7} ({} | Shuffle: {:-3} | Repeat: {:-5} | Volume: {:-2}%)",
        play_title,
        current_playback_context.device.name,
        shuffle_text,
        repeat_text,
        app.desired_volume()
      );

      if let Some(session) = &app.party_session {
        let party_label = match session.role {
          crate::infra::network::sync::PartyRole::Host => {
            format!("Party: {} listeners", session.guests.len())
          }
          crate::infra::network::sync::PartyRole::Guest => {
            format!("Party: following {}", session.host_name)
          }
        };
        title = format!("{} | {}", title, party_label);
      }

      if let Some(message) = app.status_message.as_ref() {
        title = format!("{} | {}", title, message);
      }

      let current_route = app.get_current_route();
      let highlight_state = (
        matches!(
          current_route.active_block,
          ActiveBlock::PlayBar | ActiveBlock::MiniPlayer
        ),
        matches!(
          current_route.hovered_block,
          ActiveBlock::PlayBar | ActiveBlock::MiniPlayer
        ),
      );

      let title_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .style(Style::default().bg(app.user_config.theme.playbar_background))
        .title(Span::styled(
          &title,
          get_color(highlight_state, app.user_config.theme),
        ))
        .border_style(get_color(highlight_state, app.user_config.theme));

      f.render_widget(title_block, layout_chunk);

      let (item_id, name, duration) = match track_item {
        PlayableItem::Track(track) => (
          track
            .id
            .as_ref()
            .map(|id| id.id().to_string())
            .unwrap_or_default(),
          track.name.to_owned(),
          track.duration,
        ),
        PlayableItem::Episode(episode) => (
          episode.id.id().to_string(),
          episode.name.to_owned(),
          episode.duration,
        ),
        _ => return,
      };

      // Use native track info for instant display when available (e.g., after skipping tracks)
      // Falls back to API data when native info is not available
      let (display_name, display_artists, display_duration_ms) =
        if let Some(ref native_info) = app.native_track_info {
          (
            native_info.name.clone(),
            native_info.artists_display.clone(),
            native_info.duration_ms as u64,
          )
        } else {
          let artists_str = match track_item {
            PlayableItem::Track(track) => create_artist_string(&track.artists),
            PlayableItem::Episode(episode) => format!("{} - {}", episode.name, episode.show.name),
            _ => return,
          };
          (
            name.clone(),
            artists_str,
            duration.num_milliseconds() as u64,
          )
        };

      let track_name = if app.liked_song_ids_set.contains(&item_id) {
        format!("{}{}", &app.user_config.padded_liked_icon(), display_name)
      } else {
        display_name
      };

      let lines = Text::from(Span::styled(
        display_artists,
        Style::default().fg(app.user_config.theme.playbar_text),
      ));

      let artist = Paragraph::new(lines)
        .style(Style::default().fg(app.user_config.theme.playbar_text))
        .block(
          Block::default().title(Span::styled(
            track_name,
            Style::default()
              .fg(app.user_config.theme.selected)
              .add_modifier(Modifier::BOLD),
          )),
        );
      f.render_widget(artist, artist_area);
      draw_playbar_controls(f, app, playbar_areas.controls_area);

      let progress_ms = match app.seek_ms {
        Some(seek_ms) => seek_ms,
        None => app.song_progress_ms,
      };

      let duration_std = std::time::Duration::from_millis(display_duration_ms);
      let perc = get_track_progress_percentage(progress_ms, duration_std);

      let song_progress_label = display_track_progress(progress_ms, duration_std);
      let modifier = if app.user_config.behavior.enable_text_emphasis {
        Modifier::ITALIC | Modifier::BOLD
      } else {
        Modifier::empty()
      };
      let song_progress = LineGauge::default()
        .filled_style(
          Style::default()
            .fg(app.user_config.theme.playbar_progress)
            .add_modifier(modifier),
        )
        .unfilled_style(
          Style::default()
            .fg(app.user_config.theme.playbar_background)
            .add_modifier(modifier),
        )
        .ratio(perc as f64 / 100.0)
        .filled_symbol("⣿")
        .unfilled_symbol("⣉")
        .label(Span::styled(
          &song_progress_label,
          Style::default().fg(app.user_config.theme.playbar_progress_text),
        ));
      f.render_widget(song_progress, progress_area);

      // Draw "Like" animation (heart burst) if active
      if let Some(frame) = app.liked_song_animation_frame {
        let progress = (10 - frame) as f64;
        let y_base = 20.0 + progress * 5.0; // Rise up

        let canvas = Canvas::default()
          .block(Block::default()) // No border, transparent
          .x_bounds([0.0, 100.0])
          .y_bounds([0.0, 100.0])
          .paint(|ctx| {
            let color = app.user_config.theme.selected;
            // Center heart
            ctx.print(50.0, y_base, Span::styled("♥", Style::default().fg(color)));
            // Left particle (lagging slightly)
            ctx.print(
              48.0,
              y_base - 3.0,
              Span::styled("♥", Style::default().fg(color)),
            );
            // Right particle (lagging slightly)
            ctx.print(
              52.0,
              y_base - 3.0,
              Span::styled("♥", Style::default().fg(color)),
            );
          });

        f.render_widget(canvas, layout_chunk);
      }

      #[cfg(feature = "cover-art")]
      if let Some(cover_art) = playbar_areas.cover_art {
        app.cover_art.render(f, cover_art);
      }

      drew_playbar = true;
    }
  }

  if !drew_playbar {
    if let Some(message) = app.status_message.as_ref() {
      let title_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .style(Style::default().bg(app.user_config.theme.playbar_background))
        .title(Span::styled(
          format!("Status: {}", message),
          Style::default().fg(app.user_config.theme.playbar_text),
        ))
        .border_style(Style::default().fg(app.user_config.theme.inactive));
      f.render_widget(title_block, layout_chunk);
    }
  }
}

pub fn draw_device_list(f: &mut Frame<'_>, app: &App) {
  let [instructions_area, list_area] = f
    .area()
    .layout(&Layout::vertical([Constraint::Percentage(20), Constraint::Percentage(80)]).margin(5));

  let device_instructions: Vec<Line> = vec![
        "To play tracks, please select a device. ",
        "Use `j/k` or up/down arrow keys to move up and down and <Enter> to select. ",
        "Your choice here will be cached so you can jump straight back in when you next open `spotatui`. ",
        "You can change the playback device at any time by pressing `d`.",
    ].into_iter().map(|instruction| Line::from(Span::raw(instruction))).collect();

  let instructions = Paragraph::new(device_instructions)
    .style(Style::default().fg(app.user_config.theme.text))
    .wrap(Wrap { trim: true })
    .block(
      Block::default().borders(Borders::NONE).title(Span::styled(
        "Welcome to spotatui!",
        Style::default()
          .fg(app.user_config.theme.active)
          .add_modifier(Modifier::BOLD),
      )),
    );
  f.render_widget(instructions, instructions_area);

  let no_device_message = Span::raw("No devices found: Make sure a device is active");

  let items = match &app.devices {
    Some(items) => {
      if items.devices.is_empty() {
        vec![ListItem::new(no_device_message)]
      } else {
        items
          .devices
          .iter()
          .map(|device| ListItem::new(Span::raw(&device.name)))
          .collect()
      }
    }
    None => vec![ListItem::new(no_device_message)],
  };

  let mut state = ListState::default();
  state.select(app.selected_device_index);
  let list = List::new(items)
    .block(
      Block::default()
        .title(Span::styled(
          "Devices",
          Style::default().fg(app.user_config.theme.active),
        ))
        .borders(Borders::ALL)
        .style(app.user_config.theme.base_style())
        .border_style(Style::default().fg(app.user_config.theme.inactive)),
    )
    .style(app.user_config.theme.base_style())
    .highlight_style(
      Style::default()
        .fg(app.user_config.theme.active)
        .bg(app.user_config.theme.inactive)
        .add_modifier(Modifier::BOLD),
    )
    .highlight_symbol(Line::from("▶ ").style(Style::default().fg(app.user_config.theme.active)));
  f.render_stateful_widget(list, list_area, &mut state);
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn control_hitboxes_handle_zero_sized_area() {
    assert!(playbar_control_hitboxes_in_area(Rect::new(0, 0, 0, 0)).is_empty());
    assert!(playbar_control_hitboxes_in_area(Rect::new(0, 0, 10, 0)).is_empty());
  }

  #[test]
  fn control_hitboxes_truncate_for_tiny_widths() {
    let hitboxes = playbar_control_hitboxes_in_area(Rect::new(5, 10, 8, 1));
    assert_eq!(hitboxes.len(), 1);
    assert_eq!(hitboxes[0].control, PlaybarControl::Prev);
    assert_eq!(hitboxes[0].rect, Rect::new(5, 10, 6, 1));
  }

  #[test]
  fn control_hitboxes_include_all_controls_when_wide_enough() {
    let hitboxes = playbar_control_hitboxes_in_area(Rect::new(0, 0, 200, 1));
    assert_eq!(hitboxes.len(), PLAYBAR_CONTROLS.len());
    assert_eq!(hitboxes[0].control, PlaybarControl::Prev);
    assert_eq!(
      hitboxes[PLAYBAR_CONTROLS.len() - 1].control,
      PlaybarControl::VolumeUp
    );
  }

  #[cfg(feature = "cover-art")]
  #[test]
  fn center_rect_within_centers_smaller_rect() {
    let bounds = Rect::new(10, 20, 100, 50);
    let size = Rect::new(0, 0, 80, 40);

    assert_eq!(center_rect_within(bounds, size), Rect::new(20, 25, 80, 40));
  }

  #[cfg(feature = "cover-art")]
  #[test]
  fn playbar_cover_layout_uses_default_slot_width() {
    let layout = playbar_cover_layout(Rect::new(2, 3, 100, 4), 100);

    assert_eq!(layout.slot, Rect::new(2, 3, 6, 4));
    assert_eq!(layout.image_area, Rect::new(2, 3, 6, 3));
    assert_eq!(layout.text_area, Rect::new(9, 3, 93, 4));
  }

  #[cfg(feature = "cover-art")]
  #[test]
  fn playbar_cover_layout_centers_rendered_image_in_slot() {
    let layout =
      playbar_cover_layout(Rect::new(2, 3, 100, 6), 200).with_rendered_size(Rect::new(0, 0, 8, 4));

    assert_eq!(layout.slot, Rect::new(2, 3, 12, 6));
    assert_eq!(layout.image_area, Rect::new(4, 5, 8, 4));
    assert_eq!(layout.text_area, Rect::new(15, 3, 87, 6));
  }

  #[cfg(feature = "cover-art")]
  #[test]
  fn playbar_cover_layout_bottom_aligns_smaller_rendered_image_at_max_size() {
    let layout =
      playbar_cover_layout(Rect::new(2, 3, 100, 4), 200).with_rendered_size(Rect::new(0, 0, 6, 3));

    assert_eq!(layout.image_area, Rect::new(3, 4, 6, 3));
  }

  #[cfg(feature = "cover-art")]
  #[test]
  fn cover_playbar_progress_reclaims_the_row_below_the_cover() {
    let inner = Rect::new(1, 3, 49, 4);
    let text_area = Rect::new(10, 3, 40, 4);
    let image_area = Rect::new(1, 3, 6, 3);
    let (artist_area, controls_area, progress_area) =
      split_cover_playbar_rows(inner, text_area, image_area);

    assert_eq!(artist_area, Rect::new(10, 3, 40, 2));
    assert_eq!(controls_area, Rect::new(10, 3, 40, 0));
    assert_eq!(progress_area, Rect::new(1, 6, 49, 1));
  }

  #[cfg(feature = "cover-art")]
  #[test]
  fn cover_playbar_progress_stays_beside_a_full_height_cover() {
    let inner = Rect::new(1, 3, 49, 4);
    let text_area = Rect::new(10, 3, 40, 4);
    let image_area = Rect::new(1, 3, 8, 4);
    let (artist_area, controls_area, progress_area) =
      split_cover_playbar_rows(inner, text_area, image_area);

    assert_eq!(artist_area, Rect::new(10, 3, 40, 2));
    assert_eq!(controls_area, Rect::new(10, 3, 40, 0));
    assert_eq!(progress_area, Rect::new(10, 6, 40, 1));
  }

  #[cfg(feature = "cover-art")]
  #[test]
  fn cover_playbar_rows_reserve_full_width_controls_below_the_cover() {
    let inner = Rect::new(1, 3, 109, 6);
    let text_area = Rect::new(14, 3, 96, 6);
    let image_area = Rect::new(1, 3, 10, 4);
    let (artist_area, controls_area, progress_area) =
      split_cover_playbar_rows(inner, text_area, image_area);

    assert_eq!(artist_area, Rect::new(14, 3, 96, 2));
    assert_eq!(controls_area, Rect::new(1, 7, 109, 1));
    assert_eq!(progress_area, Rect::new(1, 8, 109, 1));
  }

  #[cfg(feature = "cover-art")]
  #[test]
  fn playbar_cover_layout_scales_smaller_and_larger_sizes() {
    let smaller = playbar_cover_layout(Rect::new(0, 0, 100, 4), 50);
    let larger = playbar_cover_layout(Rect::new(0, 0, 100, 4), 200);

    assert_eq!(smaller.slot.width, 4);
    assert_eq!(smaller.text_area, Rect::new(5, 0, 95, 4));
    assert_eq!(larger.slot.width, 8);
    assert_eq!(larger.text_area, Rect::new(9, 0, 91, 4));
    assert_eq!(smaller.image_area.height, 2);
    assert_eq!(larger.image_area.height, 4);
  }

  #[cfg(feature = "cover-art")]
  #[test]
  fn scaled_cover_art_height_maps_200_to_full_available_height() {
    assert_eq!(scaled_cover_art_height(5, 25), 2);
    assert_eq!(scaled_cover_art_height(5, 100), 3);
    assert_eq!(scaled_cover_art_height(5, 200), 5);
  }

  #[cfg(feature = "cover-art")]
  #[test]
  fn playbar_cover_layout_clamps_to_tiny_playbar_area() {
    let layout = playbar_cover_layout(Rect::new(0, 0, 10, 6), 200);

    assert_eq!(layout.slot, Rect::new(0, 0, 8, 6));
    assert_eq!(layout.image_area, Rect::new(0, 0, 8, 6));
    assert_eq!(layout.text_area, Rect::new(9, 0, 1, 6));

    let zero_height = playbar_cover_layout(Rect::new(4, 5, 10, 0), 100);
    assert_eq!(zero_height.slot, Rect::new(4, 5, 0, 0));
    assert_eq!(zero_height.text_area, Rect::new(4, 5, 10, 0));
  }
}
