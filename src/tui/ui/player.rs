#[cfg(feature = "cover-art")]
use crate::core::layout::fullscreen_view_layout;
#[cfg(feature = "cover-art")]
use ratatui::layout::Alignment;

use crate::core::{
  app::{ActiveBlock, App, SourceFocus},
  layout::miniplayer_playbar_area,
  source::Source,
};
use ratatui::{
  layout::{Constraint, Layout, Position, Rect},
  style::{Modifier, Style},
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
use unicode_width::UnicodeWidthStr;

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
  /// The built-in (default) label for this control.
  const fn default_label(self) -> &'static str {
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

  /// The `playbar_control_labels` map key for this control.
  const fn config_key(self) -> &'static str {
    match self {
      Self::Prev => "prev",
      Self::PlayPause => "play_pause",
      Self::Next => "next",
      Self::Shuffle => "shuffle",
      Self::Repeat => "repeat",
      Self::Like => "like",
      Self::VolumeDown => "vol_down",
      Self::VolumeUp => "vol_up",
    }
  }

  /// The label for this control, honoring a `playbar_control_labels` override
  /// from config (falling back to the built-in default). Mouse hit-testing
  /// uses the same resolver, so hitboxes always match what's rendered.
  fn label(
    self,
    behavior: &crate::core::user_config::BehaviorConfig,
  ) -> std::borrow::Cow<'static, str> {
    match behavior.playbar_control_labels.get(self.config_key()) {
      Some(override_label) => std::borrow::Cow::Owned(override_label.clone()),
      None => std::borrow::Cow::Borrowed(self.default_label()),
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
        let (artist_area, controls_area, progress_area) = split_cover_playbar_rows(
          other,
          cover_layout.text_area,
          cover_layout.image_area,
          &app.user_config.behavior,
        );

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
fn split_cover_playbar_rows(
  inner: Rect,
  text_area: Rect,
  image_area: Rect,
  behavior: &crate::core::user_config::BehaviorConfig,
) -> (Rect, Rect, Rect) {
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

  let controls_area =
    cover_playbar_controls_area(inner, text_area, image_area, progress_area, behavior);
  let artist_area = cover_playbar_artist_area(text_area, image_area, controls_area, progress_area);

  (artist_area, controls_area, progress_area)
}

#[cfg(feature = "cover-art")]
fn cover_playbar_controls_area(
  inner: Rect,
  text_area: Rect,
  image_area: Rect,
  progress_area: Rect,
  behavior: &crate::core::user_config::BehaviorConfig,
) -> Rect {
  // Reserve room for the full control row: this only decides *whether* a controls
  // row fits, and which controls actually apply depends on the owning source
  // (`playbar_supported_controls`), which this layout pass has no `App` to ask.
  // Sizing for the widest case keeps the row appearing in exactly the same
  // situations as before, just with fewer buttons drawn in it.
  let required_width = playbar_controls_required_width(behavior, &PLAYBAR_CONTROLS);
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

/// The controls that apply to whatever currently owns playback.
///
/// Rendering and hit-testing both go through this, so a button that is drawn is
/// always one that works. They used to disagree — the row drew all eight while
/// hit-testing accepted only Play/Pause — which left inert Shuffle/Repeat
/// buttons sitting on the local playbar, inviting a click that did nothing.
fn playbar_supported_controls(app: &App) -> Vec<PlaybarControl> {
  playbar_supported_controls_for(
    app.queue_owns_playback(),
    non_spotify_source_playback_active(app),
    app.active_queueable_decoded_source(),
  )
}

/// The pure core of [`playbar_supported_controls`], taking the three facts it
/// depends on rather than an [`App`]: a real `LocalPlaybackState` needs an
/// `Arc<LocalPlayer>` holding an open audio device, so the whole support matrix
/// would otherwise be untestable.
///
/// Deliberately blind to `current_playback_context`: [`draw_playbar`] checks the
/// queue slot and every decoded source *before* it looks at the Spotify context
/// and returns early, so whenever either owns playback the playbar on screen is
/// theirs. Gating these controls on a Spotify context that may merely be cached
/// from an earlier session would offer buttons for a track the user cannot see
/// or hear.
fn playbar_supported_controls_for(
  queue_owns_playback: bool,
  non_spotify_active: bool,
  queueable_decoded: bool,
) -> Vec<PlaybarControl> {
  let mut controls = PLAYBAR_CONTROLS.to_vec();

  // The native queue slot owns the sink, so `draw_playbar` renders the *queued*
  // track over a suspended context with the mode indicators hidden. Transport
  // still works and stays offered: `App::next_track` advances the queue and
  // `App::previous_track` restarts the queued track, both via the queue router.
  // Dropped: Shuffle/Repeat have no queue-slot state to show (`App::shuffle` /
  // `App::repeat` no-op here), and Like acts on `current_playback_context` —
  // under a suspended *Spotify* context that is a different track than the one on
  // screen, so the button would save a song the user is not listening to.
  // Checked before `non_spotify_active`, which is false in exactly that case (a
  // suspended Spotify context sets no `*_playback`) and would otherwise hand back
  // all eight controls.
  if queue_owns_playback {
    controls.retain(|control| {
      !matches!(
        control,
        PlaybarControl::Like | PlaybarControl::Shuffle | PlaybarControl::Repeat
      )
    });
    return controls;
  }

  // No decoded source: the Spotify playbar is on screen and drives everything.
  if !non_spotify_active {
    return controls;
  }

  // Every decoded source routes play/pause and volume to its own player, rather
  // than the paused librespot (see `App::toggle_playback` / `App::increase_volume`).
  // Prev/Next/Shuffle/Repeat additionally need a finite track queue to act on,
  // which internet radio (an endless stream) and the native queue slot don't
  // have. Liking needs a Spotify item id, which no decoded source has.
  controls.retain(|control| match control {
    PlaybarControl::PlayPause | PlaybarControl::VolumeDown | PlaybarControl::VolumeUp => true,
    PlaybarControl::Prev
    | PlaybarControl::Next
    | PlaybarControl::Shuffle
    | PlaybarControl::Repeat => queueable_decoded,
    PlaybarControl::Like => false,
  });
  controls
}

fn playbar_control_hitboxes_in_area(
  controls_area: Rect,
  behavior: &crate::core::user_config::BehaviorConfig,
  controls: &[PlaybarControl],
) -> Vec<PlaybarControlHitbox> {
  if controls_area.width == 0 || controls_area.height == 0 {
    return Vec::new();
  }

  let required_width = playbar_controls_required_width(behavior, controls);
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
  let mut hitboxes = Vec::with_capacity(controls.len());

  for control in controls.iter().copied() {
    let label = control.label(behavior);
    let width = unicode_width::UnicodeWidthStr::width(label.as_ref()) as u16;
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

fn playbar_controls_required_width(
  behavior: &crate::core::user_config::BehaviorConfig,
  controls: &[PlaybarControl],
) -> u16 {
  controls
    .iter()
    .enumerate()
    .fold(0u16, |width, (idx, control)| {
      width.saturating_add(u16::from(idx > 0)).saturating_add(
        unicode_width::UnicodeWidthStr::width(control.label(behavior).as_ref()) as u16,
      )
    })
}

pub(crate) fn playbar_control_hitboxes(
  app: &App,
  playbar_area: Rect,
) -> Vec<(PlaybarControl, Rect)> {
  if !playbar_controls_available(app) {
    return Vec::new();
  }

  let controls_area = playbar_layout_areas(app, playbar_area).controls_area;
  playbar_control_hitboxes_in_area(
    controls_area,
    &app.user_config.behavior,
    &playbar_supported_controls(app),
  )
  .into_iter()
  .map(|hitbox| (hitbox.control, hitbox.rect))
  .collect()
}

fn playbar_controls_available(app: &App) -> bool {
  // The queue slot owns playback, so `draw_playbar` is rendering the queued track
  // and `playbar_supported_controls` offers the controls that drive it. Checked
  // first because neither fact below need hold: queueing from an idle app
  // suspends nothing, leaving every `*_playback` `None` and (for a user who never
  // started Spotify) no context either — which used to draw those controls while
  // hit-testing returned no boxes, so they were inert.
  if app.queue_owns_playback() {
    return true;
  }

  if app.current_playback_context.as_ref().is_some_and(|ctx| {
    ctx.item.is_some() || (app.is_streaming_active && app.native_device_id.is_some())
  }) {
    return true;
  }

  non_spotify_source_playback_active(app)
}

/// True when a non-Spotify source (local/subsonic/radio/youtube) currently
/// owns playback. Each source's playback field is gated behind its own
/// feature flag, so every access here is guarded to match — this must
/// compile in the slim build (no source features) as well as any single- or
/// all-sources build.
fn non_spotify_source_playback_active(app: &App) -> bool {
  // Slim builds (no source features) never reference `app` below.
  #[cfg(not(any(
    feature = "local-files",
    feature = "subsonic",
    feature = "internet-radio",
    feature = "youtube"
  )))]
  let _ = app;

  #[cfg(feature = "local-files")]
  if app.local_playback.is_some() {
    return true;
  }

  #[cfg(feature = "subsonic")]
  if app.subsonic_playback.is_some() {
    return true;
  }

  #[cfg(feature = "internet-radio")]
  if app.radio_playback.is_some() {
    return true;
  }

  #[cfg(feature = "youtube")]
  if app.youtube_playback.is_some() {
    return true;
  }

  false
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

/// Geometry of the seekable region of the playbar progress line, used to translate
/// a mouse column into an absolute playback position. Mirrors ratatui's `LineGauge`
/// layout: the left-aligned label is drawn first, then the gauge line begins one
/// column after it.
#[derive(Clone, Copy, Debug)]
pub(crate) struct PlaybarProgressLine {
  /// Row the progress line is rendered on.
  pub(crate) row: u16,
  /// First column of the gauge line (the cell after the label + 1-column gap).
  pub(crate) start: u16,
  /// Number of cells in the gauge line.
  pub(crate) width: u16,
  /// Track duration the line represents, in milliseconds.
  pub(crate) duration_ms: u32,
}

impl PlaybarProgressLine {
  /// True when `(x, y)` lands on the seekable gauge line (excludes the time label).
  pub(crate) fn contains(&self, x: u16, y: u16) -> bool {
    y == self.row && x >= self.start && x < self.start + self.width
  }

  /// True when `y` is on the progress row, regardless of column (used for drags,
  /// where the column is clamped into range rather than rejected).
  pub(crate) fn on_row(&self, y: u16) -> bool {
    y == self.row
  }

  /// Map a column to an absolute position in milliseconds, clamped to the line.
  /// The far-right cell maps to just under the full duration so a click never
  /// overshoots into the next track.
  pub(crate) fn position_at(&self, x: u16) -> u32 {
    let last = self.start + self.width.saturating_sub(1);
    let offset = x.clamp(self.start, last) - self.start;
    let fraction = f64::from(offset) / f64::from(self.width.max(1));
    (f64::from(self.duration_ms) * fraction).round() as u32
  }
}

/// Compute the seekable geometry of the playbar progress line for the current
/// playback, or `None` when nothing is playing or the line is not rendered (e.g.
/// the single-row playbar, or a terminal too narrow to fit the gauge).
pub(crate) fn playbar_progress_line(app: &App, playbar_area: Rect) -> Option<PlaybarProgressLine> {
  let item = app
    .current_playback_context
    .as_ref()
    .and_then(|ctx| ctx.item.as_ref())?;

  let progress_area = playbar_layout_areas(app, playbar_area).progress_area;
  if progress_area.width == 0 || progress_area.height == 0 {
    return None;
  }

  // Duration as shown on the playbar (native track info preferred). Mirrors
  // draw_playbar's `display_duration_ms`, so keep the two in sync (player.rs ~761).
  let duration_ms = if let Some(native_info) = &app.native_track_info {
    native_info.duration_ms
  } else {
    match item {
      PlayableItem::Track(track) => track.duration.num_milliseconds() as u32,
      PlayableItem::Episode(episode) => episode.duration.num_milliseconds() as u32,
      _ => return None,
    }
  };

  // Recreate the gauge label exactly as draw_playbar does so the computed line
  // `start` matches the rendered bar. The label reflects a pending seek if one is
  // in flight (see player.rs ~805), otherwise the current progress.
  let progress_ms = app.seek_ms.unwrap_or(app.song_progress_ms);
  let duration_std = std::time::Duration::from_millis(u64::from(duration_ms));
  let label = display_track_progress(progress_ms, duration_std);

  // LineGauge writes the label (capped at the area width), then starts the line one
  // column later: `start = label_end + 1` (see ratatui-widgets LineGauge::render).
  let label_width = (UnicodeWidthStr::width(label.as_str()) as u16).min(progress_area.width);
  let start = progress_area.x + label_width + 1;
  let right = progress_area.x + progress_area.width;
  if start >= right {
    return None;
  }

  Some(PlaybarProgressLine {
    row: progress_area.y,
    start,
    width: right - start,
    duration_ms,
  })
}

fn draw_playbar_controls(f: &mut Frame<'_>, app: &App, controls_area: Rect) {
  let controls_style = Style::default().fg(app.user_config.theme.playbar_text);
  for hitbox in playbar_control_hitboxes_in_area(
    controls_area,
    &app.user_config.behavior,
    &playbar_supported_controls(app),
  ) {
    let control = Paragraph::new(Span::styled(
      hitbox.control.label(&app.user_config.behavior).into_owned(),
      controls_style,
    ));
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
    use crate::core::app::CoverArtStatus;
    // No image is loaded: show an explicit message for the current state rather
    // than a blank pane, so "no art" always reads as a deliberate outcome.
    let message = match app.cover_art_status {
      CoverArtStatus::Loading => "Loading cover art...",
      CoverArtStatus::Unavailable => "No cover art for this source",
      CoverArtStatus::Failed => "Cover art unavailable",
      CoverArtStatus::Loaded | CoverArtStatus::NotStarted => "No cover art available",
    };
    let p = Paragraph::new(message)
      .style(Style::default().fg(app.user_config.theme.inactive))
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
            .add_modifier(app.user_config.behavior.emphasis(Modifier::BOLD)),
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
  // Read from the source-agnostic snapshot so the fullscreen cover view labels
  // the current track for every source (Spotify, native streaming, local files,
  // Subsonic, radio, YouTube), not just Spotify.
  match crate::infra::media_metadata::current_playback_snapshot(app) {
    Some(snapshot) => (
      Some(snapshot.metadata.title.clone()),
      Some(snapshot.primary_artist()),
    ),
    None => (None, None),
  }
}

/// Display snapshot for an engine playbar (local files, Subsonic, radio, or the
/// native queue slot).
///
/// Extracted from the live player so [`render_local_playbar`] is a pure function
/// of plain values and can be unit-tested with `TestBackend` (no audio device).
/// Gated to every build that can render one: the decoded sources plus the
/// native queue slot (`streaming` covers a queued Spotify track).
#[cfg(any(
  feature = "local-files",
  feature = "subsonic",
  feature = "internet-radio",
  feature = "youtube",
  feature = "streaming",
  feature = "audio-decode"
))]
struct LocalPlaybarView {
  /// Source name shown in the playbar title, e.g. `"Local"` or `"Subsonic"`.
  source_label: &'static str,
  name: String,
  artists: String,
  is_playing: bool,
  position_ms: u128,
  duration_ms: u64,
  volume_percent: u8,
  /// 1-based position in the queue and total length, e.g. `(3, 12)` => "3/12".
  /// `None` hides the indicator (e.g. a one-track session).
  queue_position: Option<(usize, usize)>,
  /// An infinite live stream (internet radio): `duration_ms` is meaningless,
  /// so the seek bar renders as a full LIVE indicator with elapsed time only.
  live: bool,
  /// Whether the decoded shuffle/repeat controls apply here. `true` for a
  /// queueable source that owns playback (Local/Subsonic/YouTube); `false` for
  /// internet radio and native queue slots, whose playbar drops the mode
  /// segments entirely rather than showing blank `Shuffle:`/`Repeat:` labels.
  show_modes: bool,
}

/// Render the playbar for an active local-file playback session.
///
/// Local playback has no Spotify `current_playback_context`; it renders from its
/// own [`App::local_playback`] state, reading progress and pause state **live**
/// from the player so they never desync from what is actually playing.
#[cfg(feature = "local-files")]
fn draw_local_playbar(f: &mut Frame<'_>, app: &App, layout_chunk: Rect) {
  let Some(local) = app.local_playback.as_ref() else {
    return;
  };
  let view = LocalPlaybarView {
    source_label: "Local",
    name: local.name.clone(),
    artists: local.artists.clone(),
    is_playing: !local.player.is_paused(),
    position_ms: local.player.position().as_millis(),
    duration_ms: local.duration_ms,
    volume_percent: app.user_config.behavior.volume_percent,
    // Only show the indicator for multi-track queues; a single file is noise.
    queue_position: (local.queue.len() > 1).then(|| (local.index + 1, local.queue.len())),
    live: false,
    show_modes: true,
  };
  render_local_playbar(f, app, layout_chunk, &view);
}

/// Render the playbar for an active Subsonic playback session, reading
/// progress/pause live from the player just like the local path.
#[cfg(feature = "subsonic")]
fn draw_subsonic_playbar(f: &mut Frame<'_>, app: &App, layout_chunk: Rect) {
  let Some(subsonic) = app.subsonic_playback.as_ref() else {
    return;
  };
  let track = subsonic.current();
  let view = LocalPlaybarView {
    source_label: "Subsonic",
    name: track.map(|t| t.name.clone()).unwrap_or_default(),
    artists: track.map(|t| t.artists.join(", ")).unwrap_or_default(),
    is_playing: !subsonic.player.is_paused(),
    position_ms: subsonic.player.position().as_millis(),
    duration_ms: track.map(|t| t.duration_ms).unwrap_or(0),
    volume_percent: app.user_config.behavior.volume_percent,
    queue_position: (subsonic.tracks.len() > 1)
      .then(|| (subsonic.index + 1, subsonic.tracks.len())),
    live: false,
    show_modes: true,
  };
  render_local_playbar(f, app, layout_chunk, &view);
}

/// Render the playbar for an active YouTube playback session, reading
/// progress/pause live from the player just like the Subsonic path.
#[cfg(feature = "youtube")]
fn draw_youtube_playbar(f: &mut Frame<'_>, app: &App, layout_chunk: Rect) {
  let Some(youtube) = app.youtube_playback.as_ref() else {
    return;
  };
  let track = youtube.current();
  let view = LocalPlaybarView {
    source_label: "YouTube",
    name: track.map(|t| t.name.clone()).unwrap_or_default(),
    artists: track.map(|t| t.artists.join(", ")).unwrap_or_default(),
    is_playing: !youtube.player.is_paused(),
    position_ms: youtube.player.position().as_millis(),
    duration_ms: track.map(|t| t.duration_ms).unwrap_or(0),
    volume_percent: app.user_config.behavior.volume_percent,
    queue_position: (youtube.tracks.len() > 1).then(|| (youtube.index + 1, youtube.tracks.len())),
    live: false,
    show_modes: true,
  };
  render_local_playbar(f, app, layout_chunk, &view);
}

/// Render the playbar for an active internet-radio session. The station name is
/// the title line and the ICY now-playing text (when the stream sends it) the
/// subtitle; elapsed time renders as a LIVE indicator since a stream is
/// infinite.
#[cfg(feature = "internet-radio")]
fn draw_radio_playbar(f: &mut Frame<'_>, app: &App, layout_chunk: Rect) {
  let Some(radio) = app.radio_playback.as_ref() else {
    return;
  };
  // Subtitle preference: live now-playing title, else genre tags from the
  // directory row, else the station's country/codec/bitrate summary.
  let artists = radio.now_playing_title().unwrap_or_else(|| {
    if radio.station.artists.is_empty() {
      radio.station.album.clone()
    } else {
      radio.station.artists.join(", ")
    }
  });
  let view = LocalPlaybarView {
    source_label: "Radio",
    name: radio.station.name.clone(),
    artists,
    is_playing: !radio.player.is_paused(),
    position_ms: radio.player.position().as_millis(),
    duration_ms: 0,
    volume_percent: app.user_config.behavior.volume_percent,
    queue_position: None,
    live: true,
    show_modes: false,
  };
  render_local_playbar(f, app, layout_chunk, &view);
}

#[cfg(any(
  feature = "local-files",
  feature = "subsonic",
  feature = "internet-radio",
  feature = "youtube",
  feature = "streaming",
  feature = "audio-decode"
))]
fn render_local_playbar(f: &mut Frame<'_>, app: &App, layout_chunk: Rect, view: &LocalPlaybarView) {
  let playbar_areas = playbar_layout_areas(app, layout_chunk);

  let play_title = if view.is_playing { "Playing" } else { "Paused" };

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

  let queue_label = match view.queue_position {
    Some((current, total)) => format!(" | {current}/{total}"),
    None => String::new(),
  };
  // Decoded (Local / Subsonic / YouTube) shuffle + repeat, worded like the
  // Spotify playbar (Off / Track / All). Each carries its own ` | Label: value`
  // prefix (values pre-padded to the `{:-3}` / `{:-5}` widths so the default
  // template reproduces today's output byte-for-byte); contexts without the
  // controls (radio, native queue slots) render them empty so the whole segment
  // — label included — disappears instead of leaking a blank control.
  use crate::infra::queue::RepeatMode;
  let (shuffle_text, repeat_text) = if view.show_modes {
    let shuffle = if app.decoded_shuffle { "On" } else { "Off" };
    let repeat = match app.decoded_repeat {
      RepeatMode::Off => "Off",
      RepeatMode::Track => "Track",
      RepeatMode::Context => "All",
    };
    (
      format!(" | Shuffle: {shuffle:<3}"),
      format!(" | Repeat: {repeat:<5}"),
    )
  } else {
    (String::new(), String::new())
  };
  // Build the title from the configurable playbar_status_source template.
  let title = app.user_config.format.playbar_status_source.render(&[
    // state — left-aligned to 7 cols (matches `{:-7}`)
    &format!("{:<7}", play_title),
    // device / source / queue
    "",
    view.source_label,
    &queue_label,
    // shuffle / repeat — self-contained ` | Label: value` segments (or empty)
    &shuffle_text,
    &repeat_text,
    // volume — left-aligned to 2 (matches `{:-2}`)
    &format!("{:<2}", view.volume_percent),
    "",
  ]);
  let mut title_spans = vec![Span::styled(
    title,
    get_color(highlight_state, app.user_config.theme),
  )];
  if let Some(message) = app.status_message.as_ref() {
    let msg_style = if app.status_message_is_error {
      Style::default().fg(app.user_config.theme.error_text)
    } else {
      get_color(highlight_state, app.user_config.theme)
    };
    title_spans.push(Span::styled(format!(" | {}", message), msg_style));
  }

  let title_block = Block::default()
    .borders(Borders::ALL)
    .border_type(BorderType::Rounded)
    .style(Style::default().bg(app.user_config.theme.playbar_background))
    .title(Line::from(title_spans))
    .border_style(get_color(highlight_state, app.user_config.theme));
  f.render_widget(title_block, layout_chunk);

  let lines = Text::from(Span::styled(
    view.artists.clone(),
    Style::default().fg(app.user_config.theme.playbar_text),
  ));
  let artist = Paragraph::new(lines)
    .style(Style::default().fg(app.user_config.theme.playbar_text))
    .block(
      Block::default().title(Span::styled(
        view.name.clone(),
        Style::default()
          .fg(app.user_config.theme.selected)
          .add_modifier(app.user_config.behavior.emphasis(Modifier::BOLD)),
      )),
    );
  f.render_widget(artist, playbar_areas.artist_area);

  draw_playbar_controls(f, app, playbar_areas.controls_area);

  // A live stream has no duration: never feed the zero into the percentage
  // math (division by zero); render a full bar with elapsed time instead.
  let (perc, song_progress_label) = if view.live {
    (
      100,
      format!(
        "LIVE \u{2022} {}",
        super::util::millis_to_minutes(view.position_ms)
      ),
    )
  } else {
    let duration_std = std::time::Duration::from_millis(view.duration_ms);
    (
      get_track_progress_percentage(view.position_ms, duration_std),
      display_track_progress(view.position_ms, duration_std),
    )
  };
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
    .filled_symbol(app.user_config.behavior.gauge_filled_icon.as_str())
    .unfilled_symbol(app.user_config.behavior.gauge_unfilled_icon.as_str())
    .label(Span::styled(
      &song_progress_label,
      Style::default().fg(app.user_config.theme.playbar_progress_text),
    ));
  f.render_widget(song_progress, playbar_areas.progress_area);

  // Paint the cover art into the slot `playbar_layout_areas` reserved — the
  // layout only carves out the space; without this the sources' playbar shows
  // a blank indent where the image belongs (Spotify's path does the same).
  #[cfg(feature = "cover-art")]
  if let Some(cover_art) = playbar_areas.cover_art {
    app.cover_art.render(f, cover_art);
  }
}

pub fn draw_playbar(f: &mut Frame<'_>, app: &App, layout_chunk: Rect) {
  // The native queue slot owns playback: render the queued track, not the
  // suspended context (whose `*_playback` is still `Some`) and not the stale
  // Spotify context (still cached when a Spotify context was suspended).
  // Checked first so the playbar always shows what is actually audible.
  #[cfg(any(feature = "local-files", feature = "subsonic", feature = "youtube"))]
  if let Some(crate::infra::queue::QueueNowPlaying::Decoded(d)) = app.queue_now.as_ref() {
    let source_label = d
      .track
      .uri
      .as_deref()
      .map(crate::core::queue::queue_item_source)
      .map(crate::core::queue::source_label)
      .unwrap_or("Queue");
    // `advancing` spans the download/decode window (the slot is published
    // before the fetch), so surface it instead of a silent frozen bar.
    let name = if d.advancing {
      format!("{} (loading\u{2026})", d.track.name)
    } else {
      d.track.name.clone()
    };
    let view = LocalPlaybarView {
      source_label,
      name,
      artists: d.track.artists.join(", "),
      is_playing: !d.player.is_paused(),
      position_ms: d.player.position().as_millis(),
      duration_ms: d.track.duration_ms,
      volume_percent: app.user_config.behavior.volume_percent,
      queue_position: None,
      live: false,
      // The native queue ignores the decoded shuffle/repeat modes (they belong
      // to the suspended source resumed once the queue drains), so hide them.
      show_modes: false,
    };
    render_local_playbar(f, app, layout_chunk, &view);
    return;
  }
  // A queued Spotify track plays through librespot: progress and play-state
  // come from the native event stream, metadata from the queue slot.
  #[cfg(feature = "streaming")]
  if let Some(crate::infra::queue::QueueNowPlaying::Spotify { track }) = app.queue_now.as_ref() {
    let view = LocalPlaybarView {
      source_label: "Spotify",
      name: track.name.clone(),
      artists: track.artists.join(", "),
      is_playing: app.native_is_playing.unwrap_or(true),
      position_ms: app.song_progress_ms,
      duration_ms: track.duration_ms,
      volume_percent: app.user_config.behavior.volume_percent,
      queue_position: None,
      live: false,
      show_modes: false,
    };
    render_local_playbar(f, app, layout_chunk, &view);
    return;
  }

  // Local-file playback owns the session and has no Spotify context to render
  // from, so it takes a dedicated path.
  #[cfg(feature = "local-files")]
  if app.local_playback.is_some() {
    draw_local_playbar(f, app, layout_chunk);
    return;
  }

  // Subsonic playback likewise renders from its own session state.
  #[cfg(feature = "subsonic")]
  if app.subsonic_playback.is_some() {
    draw_subsonic_playbar(f, app, layout_chunk);
    return;
  }

  // Internet radio likewise renders from its own session state.
  #[cfg(feature = "internet-radio")]
  if app.radio_playback.is_some() {
    draw_radio_playbar(f, app, layout_chunk);
    return;
  }

  // YouTube likewise renders from its own session state.
  #[cfg(feature = "youtube")]
  if app.youtube_playback.is_some() {
    draw_youtube_playbar(f, app, layout_chunk);
    return;
  }

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

      // Build the title from the configurable playbar_status template.
      // Values are pre-padded to match the original `{:-N}` format widths so
      // the default template reproduces today's output byte-for-byte.
      let party_segment = if let Some(session) = &app.party_session {
        match session.role {
          crate::infra::network::sync::PartyRole::Host => {
            format!(" | Party: {} listeners", session.guests.len())
          }
          crate::infra::network::sync::PartyRole::Guest => {
            format!(" | Party: following {}", session.host_name)
          }
        }
      } else {
        String::new()
      };
      let title = app.user_config.format.playbar_status.render(&[
        // state — left-aligned to 7 cols (matches `{:-7}`)
        &format!("{:<7}", play_title),
        // device
        &current_playback_context.device.name,
        // source / queue — unused in the Spotify template
        "",
        "",
        // shuffle — left-aligned to 3 (matches `{:-3}`)
        &format!("{:<3}", shuffle_text),
        // repeat — left-aligned to 5 (matches `{:-5}`)
        &format!("{:<5}", repeat_text),
        // volume — left-aligned to 2 (matches `{:-2}`)
        &format!("{:<2}", app.desired_volume()),
        // party — pre-composed optional segment
        &party_segment,
      ]);

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

      let mut title_spans = vec![Span::styled(
        title,
        get_color(highlight_state, app.user_config.theme),
      )];
      if let Some(message) = app.status_message.as_ref() {
        let msg_style = if app.status_message_is_error {
          Style::default().fg(app.user_config.theme.error_text)
        } else {
          get_color(highlight_state, app.user_config.theme)
        };
        title_spans.push(Span::styled(format!(" | {}", message), msg_style));
      }
      for seg in app.plugin_playbar_segments.values() {
        title_spans.push(Span::styled(
          format!(" | {}", seg),
          Style::default().fg(app.user_config.theme.playbar_text),
        ));
      }

      let title_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .style(Style::default().bg(app.user_config.theme.playbar_background))
        .title(Line::from(title_spans))
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
        format!("{}{}", app.user_config.padded_liked_icon(), display_name)
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
              .add_modifier(app.user_config.behavior.emphasis(Modifier::BOLD)),
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
        .filled_symbol(app.user_config.behavior.gauge_filled_icon.as_str())
        .unfilled_symbol(app.user_config.behavior.gauge_unfilled_icon.as_str())
        .label(Span::styled(
          &song_progress_label,
          Style::default().fg(app.user_config.theme.playbar_progress_text),
        ));
      f.render_widget(song_progress, progress_area);

      // Draw "Like" animation (heart burst) if active
      if let Some(frame) = app.liked_song_animation_frame {
        let total_frames = app.user_config.behavior.like_animation_frames.max(1);
        let progress = (total_frames.saturating_sub(frame)) as f64;
        let y_base = 20.0 + progress * 5.0; // Rise up
        let heart = app.user_config.behavior.liked_icon.clone();
        let heart_color = app.user_config.theme.selected;

        let canvas = Canvas::default()
          .block(Block::default()) // No border, transparent
          .x_bounds([0.0, 100.0])
          .y_bounds([0.0, 100.0])
          .paint(move |ctx| {
            // Center heart
            ctx.print(
              50.0,
              y_base,
              Span::styled(heart.clone(), Style::default().fg(heart_color)),
            );
            // Left particle (lagging slightly)
            ctx.print(
              48.0,
              y_base - 3.0,
              Span::styled(heart.clone(), Style::default().fg(heart_color)),
            );
            // Right particle (lagging slightly)
            ctx.print(
              52.0,
              y_base - 3.0,
              Span::styled(heart.clone(), Style::default().fg(heart_color)),
            );
          });

        f.render_widget(canvas, layout_chunk);
      }

      #[cfg(feature = "cover-art")]
      if let Some(cover_art) = playbar_areas.cover_art {
        app.cover_art.render(f, cover_art);
      }

      drew_playbar = true;
    } else if app.is_streaming_active && app.native_device_id.is_some() {
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
      let title = format!(
        "Ready   ({} | Shuffle: {:-3} | Repeat: {:-5} | Volume: {:-2}%)",
        current_playback_context.device.name,
        shuffle_text,
        repeat_text,
        app.desired_volume()
      );
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
      let mut title_spans = vec![Span::styled(
        title,
        get_color(highlight_state, app.user_config.theme),
      )];
      if let Some(message) = app.status_message.as_ref() {
        let msg_style = if app.status_message_is_error {
          Style::default().fg(app.user_config.theme.error_text)
        } else {
          get_color(highlight_state, app.user_config.theme)
        };
        title_spans.push(Span::styled(format!(" | {}", message), msg_style));
      }

      let title_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .style(Style::default().bg(app.user_config.theme.playbar_background))
        .title(Line::from(title_spans))
        .border_style(get_color(highlight_state, app.user_config.theme));

      f.render_widget(title_block, layout_chunk);
      f.render_widget(
        Paragraph::new("No active playback")
          .style(Style::default().fg(app.user_config.theme.playbar_text)),
        artist_area,
      );
      draw_playbar_controls(f, app, playbar_areas.controls_area);
      drew_playbar = true;
    }
  }

  if !drew_playbar {
    if let Some(message) = app.status_message.as_ref() {
      let msg_style = if app.status_message_is_error {
        Style::default().fg(app.user_config.theme.error_text)
      } else {
        Style::default().fg(app.user_config.theme.playbar_text)
      };
      let title_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .style(Style::default().bg(app.user_config.theme.playbar_background))
        .title(Span::styled(format!("Status: {}", message), msg_style))
        .border_style(Style::default().fg(app.user_config.theme.inactive));
      f.render_widget(title_block, layout_chunk);
    }
  }
}

/// The combined Source & Device picker (the `d` screen): a Source panel
/// (Spotify / Local Files) stacked above the existing Spotify Connect device
/// list. `Tab` toggles focus; the Devices panel is dimmed when Local is active.
pub fn draw_device_list(f: &mut Frame<'_>, app: &App) {
  // A small margin (rather than the device screen's old 5) keeps the fixed
  // instructions + Source panel from squeezing the Devices list off-screen on
  // shorter terminals: instructions(7) + source(4) + devices(>=3) fits from ~18
  // rows up.
  let [instructions_area, source_area, devices_area] = f.area().layout(
    &Layout::vertical([
      Constraint::Length(7),
      Constraint::Length(Source::ALL.len() as u16 + 2),
      Constraint::Min(3),
    ])
    .margin(2),
  );

  let move_instructions = format!(
    "Use `{}`/`{}` or arrow keys to move, `Tab` to switch panels, <Enter> to select.",
    app.user_config.keys.move_down, app.user_config.keys.move_up,
  );
  let instructions_text: Vec<Line> = vec![
    Line::from(Span::raw(
      "Choose your music source and Spotify playback device.",
    )),
    Line::from(Span::raw(move_instructions)),
    Line::from(Span::raw(
      "Your choices are cached so you can jump straight back in when you next open `spotatui`.",
    )),
    Line::from(Span::raw("Reopen this screen any time by pressing `d`.")),
  ];

  let instructions = Paragraph::new(instructions_text)
    .style(Style::default().fg(app.user_config.theme.text))
    .wrap(Wrap { trim: true })
    .block(
      Block::default().borders(Borders::NONE).title(Span::styled(
        "Welcome to spotatui!",
        Style::default()
          .fg(app.user_config.theme.active)
          .add_modifier(app.user_config.behavior.emphasis(Modifier::BOLD)),
      )),
    );
  f.render_widget(instructions, instructions_area);

  // --- Source panel ---
  let source_focused = app.source_device_focus == SourceFocus::Source;
  let source_border = if source_focused {
    app.user_config.theme.active
  } else {
    app.user_config.theme.inactive
  };
  let source_items: Vec<ListItem> = Source::ALL
    .iter()
    .map(|s| {
      let is_active = *s == app.active_source;
      let marker = if is_active {
        app.user_config.behavior.active_source_icon.as_str()
      } else {
        " "
      };
      let suffix = if is_active { "  (active)" } else { "" };
      let style = if is_active {
        Style::default()
          .fg(app.user_config.theme.active)
          .add_modifier(app.user_config.behavior.emphasis(Modifier::BOLD))
      } else {
        app.user_config.theme.base_style()
      };
      ListItem::new(Span::styled(
        format!("{} {}{}", marker, s.label(), suffix),
        style,
      ))
    })
    .collect();
  let mut source_state = ListState::default();
  // Only the focused panel shows the moving cursor; the active source is always
  // marked with `●` regardless of focus.
  if source_focused {
    source_state.select(Some(app.source_list_index));
  }
  let source_list = List::new(source_items)
    .block(
      Block::default()
        .title(Span::styled("Source", Style::default().fg(source_border)))
        .borders(Borders::ALL)
        .style(app.user_config.theme.base_style())
        .border_style(Style::default().fg(source_border)),
    )
    .style(app.user_config.theme.base_style())
    .highlight_style(
      Style::default()
        .fg(app.user_config.theme.active)
        .bg(app.user_config.theme.inactive)
        .add_modifier(app.user_config.behavior.emphasis(Modifier::BOLD)),
    )
    .highlight_symbol(Line::from("▶ ").style(Style::default().fg(app.user_config.theme.active)));
  f.render_stateful_widget(source_list, source_area, &mut source_state);

  // --- Devices panel (Spotify Connect only) ---
  // Dimmed under any non-Spotify source (Local, Subsonic): device transfer is a
  // Spotify Connect feature.
  let non_spotify_active = app.active_source != Source::Spotify;
  let devices_focused = app.source_device_focus == SourceFocus::Devices && !non_spotify_active;
  let devices_color = if devices_focused {
    app.user_config.theme.active
  } else {
    app.user_config.theme.inactive
  };
  let devices_title = if non_spotify_active {
    "Devices (Spotify only)"
  } else {
    "Devices"
  };
  let device_text_style = if non_spotify_active {
    Style::default().fg(app.user_config.theme.inactive)
  } else {
    app.user_config.theme.base_style()
  };

  let no_device_message = Span::raw("No devices found: Make sure a device is active");
  let items: Vec<ListItem> = match &app.devices {
    Some(payload) if !payload.devices.is_empty() => payload
      .devices
      .iter()
      .map(|device| ListItem::new(Span::raw(device.name.clone())))
      .collect(),
    _ => vec![ListItem::new(no_device_message)],
  };

  let mut state = ListState::default();
  if devices_focused {
    state.select(app.selected_device_index);
  }
  let device_list = List::new(items)
    .block(
      Block::default()
        .title(Span::styled(
          devices_title,
          Style::default().fg(devices_color),
        ))
        .borders(Borders::ALL)
        .style(device_text_style)
        .border_style(Style::default().fg(devices_color)),
    )
    .style(device_text_style)
    .highlight_style(
      Style::default()
        .fg(app.user_config.theme.active)
        .bg(app.user_config.theme.inactive)
        .add_modifier(app.user_config.behavior.emphasis(Modifier::BOLD)),
    )
    .highlight_symbol(Line::from("▶ ").style(Style::default().fg(app.user_config.theme.active)));
  f.render_stateful_widget(device_list, devices_area, &mut state);
}

#[cfg(test)]
mod tests {
  use super::*;
  use chrono::Utc;
  use rspotify::model::{
    context::{Actions, CurrentPlaybackContext},
    device::Device,
    enums::{CurrentlyPlayingType, RepeatState},
    DeviceType,
  };

  #[allow(deprecated)]
  fn idle_native_app() -> App {
    let mut app = App::default();
    app.is_streaming_active = true;
    app.native_device_id = Some("native-device".to_string());
    app.current_playback_context = Some(CurrentPlaybackContext {
      device: Device {
        id: Some("native-device".to_string()),
        is_active: true,
        is_private_session: false,
        is_restricted: false,
        name: "spotatui".to_string(),
        _type: DeviceType::Computer,
        volume_percent: Some(50),
      },
      repeat_state: RepeatState::Off,
      shuffle_state: false,
      context: None,
      timestamp: Utc::now(),
      progress: None,
      is_playing: false,
      item: None,
      currently_playing_type: CurrentlyPlayingType::Unknown,
      actions: Actions::default(),
    });
    app.set_current_route_state(Some(ActiveBlock::PlayBar), Some(ActiveBlock::PlayBar));
    app
  }

  #[test]
  fn spotify_playback_keeps_every_playbar_control() {
    // No decoded source owns playback, so the Spotify playbar is on screen.
    assert_eq!(
      playbar_supported_controls_for(false, false, false),
      PLAYBAR_CONTROLS.to_vec()
    );
  }

  #[test]
  fn a_cached_spotify_context_does_not_resurrect_controls_for_a_decoded_source() {
    // Regression: this gate used to bail out to the full control set whenever
    // `current_playback_context` was `Some`, which is the common case for anyone
    // who has ever signed in to Spotify — so local files got all eight buttons
    // back, including Like. `draw_playbar` consults every decoded source before
    // the Spotify context and returns early, so the context says nothing about
    // which playbar is actually on screen and must not be consulted here.
    let local = playbar_supported_controls_for(false, true, true);
    assert!(!local.contains(&PlaybarControl::Like));
    let radio = playbar_supported_controls_for(false, true, false);
    assert!(!radio.contains(&PlaybarControl::Shuffle));
  }

  #[test]
  fn queueable_decoded_source_gets_transport_and_modes_but_not_like() {
    // Regression: Shuffle/Repeat were drawn but not clickable for local files,
    // because the row rendered every control while hit-testing kept only
    // Play/Pause. Both now come from this list, so a drawn button always works.
    let controls = playbar_supported_controls_for(false, true, true);
    for expected in [
      PlaybarControl::Prev,
      PlaybarControl::PlayPause,
      PlaybarControl::Next,
      PlaybarControl::Shuffle,
      PlaybarControl::Repeat,
      PlaybarControl::VolumeDown,
      PlaybarControl::VolumeUp,
    ] {
      assert!(
        controls.contains(&expected),
        "{expected:?} routes to the decoded source, so it should be offered: {controls:?}"
      );
    }
    // Liking needs a Spotify item id, which a decoded source has no way to supply.
    assert!(!controls.contains(&PlaybarControl::Like));
  }

  #[test]
  fn radio_only_gets_the_controls_a_live_stream_can_honour() {
    // Radio reports `queueable_decoded = false`: there is no finite track list to
    // skip through or shuffle, so offering those buttons would invite a click
    // that falls through to the wrong player.
    let controls = playbar_supported_controls_for(false, true, false);
    assert_eq!(
      controls,
      vec![
        PlaybarControl::PlayPause,
        PlaybarControl::VolumeDown,
        PlaybarControl::VolumeUp,
      ]
    );
  }

  #[test]
  fn queue_slot_drops_like_and_the_modes_but_keeps_transport() {
    // Regression: a queued track playing over a suspended *Spotify* context sets
    // no `*_playback`, so `non_spotify_active` was false and the gate handed back
    // all eight controls over the queue slot's playbar — whose Like button saves
    // `current_playback_context`'s item, i.e. the suspended Spotify track rather
    // than the queued one on screen. Both suspended-context shapes must agree.
    //
    // Prev/Next stay: unlike radio, the queue *can* honour them (`next_track`
    // dispatches AdvanceNativeQueue, `previous_track` restarts the queued track),
    // so dropping them would hide controls that work.
    let over_spotify = playbar_supported_controls_for(true, false, false);
    let over_decoded = playbar_supported_controls_for(true, true, false);
    let expected = vec![
      PlaybarControl::Prev,
      PlaybarControl::PlayPause,
      PlaybarControl::Next,
      PlaybarControl::VolumeDown,
      PlaybarControl::VolumeUp,
    ];
    assert_eq!(over_spotify, expected);
    assert_eq!(over_decoded, expected);
  }

  #[test]
  fn control_hitboxes_handle_zero_sized_area() {
    let behavior = crate::core::user_config::UserConfig::new().behavior;
    assert!(
      playbar_control_hitboxes_in_area(Rect::new(0, 0, 0, 0), &behavior, &PLAYBAR_CONTROLS)
        .is_empty()
    );
    assert!(
      playbar_control_hitboxes_in_area(Rect::new(0, 0, 10, 0), &behavior, &PLAYBAR_CONTROLS)
        .is_empty()
    );
  }

  #[test]
  fn control_hitboxes_truncate_for_tiny_widths() {
    let behavior = crate::core::user_config::UserConfig::new().behavior;
    let hitboxes =
      playbar_control_hitboxes_in_area(Rect::new(5, 10, 8, 1), &behavior, &PLAYBAR_CONTROLS);
    assert_eq!(hitboxes.len(), 1);
    assert_eq!(hitboxes[0].control, PlaybarControl::Prev);
    assert_eq!(hitboxes[0].rect, Rect::new(5, 10, 6, 1));
  }

  #[test]
  fn control_hitboxes_include_all_controls_when_wide_enough() {
    let behavior = crate::core::user_config::UserConfig::new().behavior;
    let hitboxes =
      playbar_control_hitboxes_in_area(Rect::new(0, 0, 200, 1), &behavior, &PLAYBAR_CONTROLS);
    assert_eq!(hitboxes.len(), PLAYBAR_CONTROLS.len());
    assert_eq!(hitboxes[0].control, PlaybarControl::Prev);
    assert_eq!(
      hitboxes[PLAYBAR_CONTROLS.len() - 1].control,
      PlaybarControl::VolumeUp
    );
  }

  #[test]
  fn playbar_control_hitboxes_include_controls_for_idle_native_device() {
    let app = idle_native_app();
    let hitboxes = playbar_control_hitboxes(&app, Rect::new(0, 0, 200, 6));

    assert_eq!(hitboxes.len(), PLAYBAR_CONTROLS.len());
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
    let behavior = crate::core::user_config::UserConfig::new().behavior;
    let inner = Rect::new(1, 3, 49, 4);
    let text_area = Rect::new(10, 3, 40, 4);
    let image_area = Rect::new(1, 3, 6, 3);
    let (artist_area, controls_area, progress_area) =
      split_cover_playbar_rows(inner, text_area, image_area, &behavior);

    assert_eq!(artist_area, Rect::new(10, 3, 40, 2));
    assert_eq!(controls_area, Rect::new(10, 3, 40, 0));
    assert_eq!(progress_area, Rect::new(1, 6, 49, 1));
  }

  #[cfg(feature = "cover-art")]
  #[test]
  fn cover_playbar_progress_stays_beside_a_full_height_cover() {
    let behavior = crate::core::user_config::UserConfig::new().behavior;
    let inner = Rect::new(1, 3, 49, 4);
    let text_area = Rect::new(10, 3, 40, 4);
    let image_area = Rect::new(1, 3, 8, 4);
    let (artist_area, controls_area, progress_area) =
      split_cover_playbar_rows(inner, text_area, image_area, &behavior);

    assert_eq!(artist_area, Rect::new(10, 3, 40, 2));
    assert_eq!(controls_area, Rect::new(10, 3, 40, 0));
    assert_eq!(progress_area, Rect::new(10, 6, 40, 1));
  }

  #[cfg(feature = "cover-art")]
  #[test]
  fn cover_playbar_rows_reserve_full_width_controls_below_the_cover() {
    let behavior = crate::core::user_config::UserConfig::new().behavior;
    let inner = Rect::new(1, 3, 109, 6);
    let text_area = Rect::new(14, 3, 96, 6);
    let image_area = Rect::new(1, 3, 10, 4);
    let (artist_area, controls_area, progress_area) =
      split_cover_playbar_rows(inner, text_area, image_area, &behavior);

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

  /// Collect every rendered cell symbol in `area` into one string for substring
  /// assertions.
  #[cfg(feature = "local-files")]
  fn rendered_text(area: Rect, view: &LocalPlaybarView) -> String {
    use ratatui::{backend::TestBackend, Terminal};

    let app = App::default();
    let mut terminal = Terminal::new(TestBackend::new(area.width, area.height)).unwrap();
    terminal
      .draw(|f| render_local_playbar(f, &app, area, view))
      .unwrap();
    let buffer = terminal.backend().buffer();
    (0..area.height)
      .flat_map(|y| (0..area.width).map(move |x| (x, y)))
      .filter_map(|(x, y)| buffer.cell((x, y)).map(|c| c.symbol().to_string()))
      .collect()
  }

  /// Regression guard for the "blank playbar / frozen progress" bugs: with a
  /// live position the renderer must show the track name, the playing state, and
  /// the elapsed/total time — none of which require an audio device to verify.
  #[cfg(feature = "local-files")]
  #[test]
  fn local_playbar_renders_name_state_and_progress() {
    let view = LocalPlaybarView {
      source_label: "Local",
      name: "My Local Song".to_string(),
      artists: "Some Artist".to_string(),
      is_playing: true,
      position_ms: 60_000,  // 1:00
      duration_ms: 311_811, // 5:11
      volume_percent: 80,
      queue_position: Some((3, 12)),
      live: false,
      show_modes: true,
    };
    let content = rendered_text(Rect::new(0, 0, 160, 6), &view);

    assert!(
      content.contains("My Local Song"),
      "track name should render: {content}"
    );
    assert!(
      content.contains("Shuffle:") && content.contains("Repeat:"),
      "a queueable source should show the shuffle/repeat controls: {content}"
    );
    assert!(
      content.contains("Playing"),
      "should show Playing: {content}"
    );
    assert!(
      content.contains("1:00"),
      "elapsed should show 1:00 at a 60s position: {content}"
    );
    assert!(
      content.contains("5:11"),
      "total duration should show 5:11: {content}"
    );
    assert!(
      content.contains("3/12"),
      "queue position should render for a multi-track queue: {content}"
    );
  }

  #[cfg(feature = "local-files")]
  #[test]
  fn local_playbar_shows_paused_state() {
    let view = LocalPlaybarView {
      source_label: "Local",
      name: "Track".to_string(),
      artists: "Artist".to_string(),
      is_playing: false,
      position_ms: 0,
      duration_ms: 200_000,
      volume_percent: 50,
      queue_position: None,
      live: false,
      show_modes: true,
    };
    let content = rendered_text(Rect::new(0, 0, 160, 6), &view);
    assert!(content.contains("Paused"), "should show Paused: {content}");
    assert!(
      !content.contains('/') || !content.contains("1/1"),
      "a single-track session should not show a queue indicator: {content}"
    );
  }

  /// A live (radio) view must render the station, the ICY subtitle and a LIVE
  /// elapsed label — and must not divide by the zero duration (which would show
  /// a bogus `0:00/0:00` countdown instead).
  #[cfg(feature = "local-files")]
  #[test]
  fn live_playbar_renders_live_label_instead_of_duration() {
    let view = LocalPlaybarView {
      source_label: "Radio",
      name: "SomaFM Groove Salad".to_string(),
      artists: "Boards of Canada - Olson".to_string(),
      is_playing: true,
      position_ms: 83_000, // 1:23 listening time
      duration_ms: 0,      // the LIVE sentinel
      volume_percent: 80,
      queue_position: None,
      live: true,
      show_modes: false,
    };
    let content = rendered_text(Rect::new(0, 0, 160, 6), &view);

    assert!(
      content.contains("SomaFM Groove Salad"),
      "station name should render: {content}"
    );
    assert!(
      !content.contains("Shuffle:") && !content.contains("Repeat:"),
      "radio has no queue, so the shuffle/repeat controls must stay hidden: {content}"
    );
    assert!(
      content.contains("Boards of Canada - Olson"),
      "ICY now-playing should render as the subtitle: {content}"
    );
    assert!(
      content.contains("LIVE") && content.contains("1:23"),
      "progress should render as LIVE with elapsed time: {content}"
    );
    assert!(
      !content.contains("0:00"),
      "a live stream must not render a zero duration countdown: {content}"
    );
  }

  fn render_picker(app: &App, w: u16, h: u16) -> String {
    use ratatui::{backend::TestBackend, Terminal};
    let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
    terminal.draw(|f| draw_device_list(f, app)).unwrap();
    let buffer = terminal.backend().buffer();
    (0..h)
      .flat_map(|y| (0..w).map(move |x| (x, y)))
      .filter_map(|(x, y)| buffer.cell((x, y)).map(|c| c.symbol().to_string()))
      .collect()
  }

  #[test]
  fn source_picker_lists_both_sources_and_marks_active() {
    let app = App::default(); // Spotify is the default active source
    let content = render_picker(&app, 60, 28);
    assert!(
      content.contains("Spotify"),
      "Spotify source row should render: {content}"
    );
    assert!(
      content.contains("Local Files"),
      "Local Files source row should render: {content}"
    );
    assert!(
      content.contains("(active)"),
      "the active source should be marked: {content}"
    );
    // Spotify active: the Devices panel is the normal, non-dimmed title.
    assert!(
      !content.contains("Spotify only"),
      "devices title should be plain when Spotify is active: {content}"
    );
  }

  #[test]
  fn source_picker_devices_title_notes_spotify_only_under_local() {
    let mut app = App::default();
    app.active_source = Source::Local;
    let content = render_picker(&app, 60, 28);
    assert!(
      content.contains("Spotify only"),
      "devices title should note Spotify-only when Local is active: {content}"
    );
  }

  #[test]
  fn source_picker_keeps_source_panel_on_short_terminal() {
    // Fixed instructions + Source panel must not squeeze the Source list off a
    // short terminal (the reason the margin was trimmed from 5 to 2).
    let app = App::default();
    let content = render_picker(&app, 40, 18);
    assert!(
      content.contains("Spotify") && content.contains("Local Files"),
      "both source rows should still render at 40x18: {content}"
    );
  }
}
