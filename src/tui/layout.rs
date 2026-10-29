use crate::core::app::{ActiveBlock, App};
use crate::core::input::Arrow;
use crate::core::pane::{Pane, PaneNode, SplitAxis};
use crate::core::state::RuntimeState;
use crate::core::user_config::BehaviorConfig;
use ratatui::layout::{Constraint, Layout, Margin, Rect};

pub const COMPACT_TOP_ROW_THRESHOLD: u16 = 60;
const COMPACT_HELP_WIDTH: u16 = 6;
const COMPACT_SETTINGS_WIDTH: u16 = 10;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MainLayoutAreas {
  pub input: Option<Rect>,
  pub help: Option<Rect>,
  pub settings: Option<Rect>,
  pub playbar: Rect,
  /// The full sidebar (user block) area, including the input row in wide layout.
  pub sidebar: Rect,
  pub library: Rect,
  pub playlists: Rect,
  pub content: Rect,
}

pub fn small_terminal_width(behavior: &BehaviorConfig) -> u16 {
  behavior.small_terminal_width.max(1)
}

pub fn small_terminal_height(behavior: &BehaviorConfig) -> u16 {
  behavior.small_terminal_height.max(1)
}

pub fn main_layout_margin(app: &App) -> u16 {
  if app.view.size.height > small_terminal_height(&app.user_config.behavior) {
    1
  } else {
    0
  }
}

pub fn is_wide_layout(app: &App) -> bool {
  match main_root(app) {
    Some(MainRoot::Full(root, _)) => is_wide_width(app, root.width),
    Some(MainRoot::Hidden { .. }) => false,
    None => is_wide_width(app, app.view.size.width),
  }
}

/// Whether `Main` is hidden and only its search row and playbar show beside the panes.
pub fn main_hidden(app: &App) -> bool {
  matches!(main_root(app), Some(MainRoot::Hidden { .. }))
}

fn is_wide_width(app: &App, width: u16) -> bool {
  width >= small_terminal_width(&app.user_config.behavior)
    && !app.user_config.behavior.enforce_wide_search_bar
}

const MIN_PANE_WIDTH: u16 = 12;
const MIN_PANE_HEIGHT: u16 = 4;
const MIN_MAIN_WIDTH: u16 = 40;
const MIN_MAIN_HEIGHT: u16 = 12;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PaneLayout {
  pub panes: Vec<(Pane, Rect)>,
  pub borders: Vec<PaneBorder>,
  /// The search, help and settings row and the playbar while `Main` is hidden.
  pub hidden_main: Option<(Rect, Rect)>,
}

impl PaneLayout {
  pub fn area(&self, pane: Pane) -> Option<Rect> {
    self
      .panes
      .iter()
      .find(|(leaf, _)| *leaf == pane)
      .map(|(_, area)| *area)
  }
}

/// The line between the two children of the `index`th split (pre-order).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PaneBorder {
  pub index: usize,
  pub axis: SplitAxis,
  pub parent: Rect,
  pub first: Rect,
}

impl PaneBorder {
  /// The last cell of the first child and the first cell of the second along the axis.
  pub fn is_hit(&self, column: u16, row: u16) -> bool {
    let inside = |pos: u16, start: u16, len: u16| pos >= start && pos < start.saturating_add(len);
    match self.axis {
      SplitAxis::Horizontal => {
        let edge = self.first.x + self.first.width;
        inside(row, self.parent.y, self.parent.height) && (column + 1 == edge || column == edge)
      }
      SplitAxis::Vertical => {
        let edge = self.first.y + self.first.height;
        inside(column, self.parent.x, self.parent.width) && (row + 1 == edge || row == edge)
      }
    }
  }

  /// The share of the first child when the border sits at this cell.
  pub fn percent_at(&self, column: u16, row: u16) -> u8 {
    let (pos, start, len) = match self.axis {
      SplitAxis::Horizontal => (column, self.parent.x, self.parent.width),
      SplitAxis::Vertical => (row, self.parent.y, self.parent.height),
    };
    let offset = u32::from(pos.saturating_sub(start)) + 1;
    (offset * 100 / u32::from(len.max(1))).min(100) as u8
  }
}

/// The detached panes and `Main` with their rects, or `None` when only `Main`
/// shows: nothing is detached, or a pane would not fit the terminal. A hidden
/// `Main` leaves its search, help and settings row and its playbar beside the panes.
pub fn compute_pane_layout(app: &App) -> Option<PaneLayout> {
  compute_pane_layout_for(app, app.pane_layout())
}

/// `compute_pane_layout` for a tree that is not saved yet.
pub fn compute_pane_layout_for(app: &App, tree: &PaneNode) -> Option<PaneLayout> {
  if *tree == PaneNode::default() || app.view.size.width == 0 || app.view.size.height == 0 {
    return None;
  }
  let margin = main_layout_margin(app);
  let mut root = Rect::new(0, 0, app.view.size.width, app.view.size.height).inner(Margin {
    horizontal: margin,
    vertical: margin,
  });
  let mut hidden_main = None;
  if !tree.contains(Pane::Main) {
    let playbar = playbar_constraint(&app.runtime_state);
    let (bar, rest, playbar) = if app.user_config.behavior.playbar_position == "top" {
      let [playbar, bar, rest] = root.layout(&Layout::vertical([
        playbar,
        Constraint::Length(3),
        Constraint::Min(0),
      ]));
      (bar, rest, playbar)
    } else {
      let [bar, rest, playbar] = root.layout(&Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(0),
        playbar,
      ]));
      (bar, rest, playbar)
    };
    hidden_main = Some((bar, playbar));
    root = rest;
  }
  let mut layout = PaneLayout {
    panes: Vec::new(),
    borders: Vec::new(),
    hidden_main,
  };
  split_pane_node(tree, root, &mut layout, &mut 0);
  let fits = layout.panes.iter().all(|(pane, area)| {
    let (width, height) = if *pane == Pane::Main {
      (MIN_MAIN_WIDTH, MIN_MAIN_HEIGHT)
    } else {
      (MIN_PANE_WIDTH, MIN_PANE_HEIGHT)
    };
    area.width >= width && area.height >= height
  });
  fits.then_some(layout)
}

fn split_pane_node(node: &PaneNode, area: Rect, layout: &mut PaneLayout, splits: &mut usize) {
  match node {
    PaneNode::Leaf(pane) => layout.panes.push((*pane, area)),
    PaneNode::Split(split) => {
      let percent = u16::from(split.percent);
      let constraints = [
        Constraint::Percentage(percent),
        Constraint::Percentage(100 - percent),
      ];
      let [first, second] = match split.axis {
        SplitAxis::Horizontal => area.layout(&Layout::horizontal(constraints)),
        SplitAxis::Vertical => area.layout(&Layout::vertical(constraints)),
      };
      layout.borders.push(PaneBorder {
        index: *splits,
        axis: split.axis,
        parent: area,
        first,
      });
      *splits += 1;
      split_pane_node(&split.first, first, layout, splits);
      split_pane_node(&split.second, second, layout, splits);
    }
  }
}

enum MainRoot {
  /// The rect of all of `Main`, with the margin applied inside it.
  Full(Rect, u16),
  /// The search, help and settings row and the playbar of a hidden `Main`.
  Hidden { bar: Rect, playbar: Rect },
}

fn main_root(app: &App) -> Option<MainRoot> {
  if app.view.size.width == 0 || app.view.size.height == 0 {
    return None;
  }
  match compute_pane_layout(app) {
    Some(layout) => match layout.area(Pane::Main) {
      Some(area) => Some(MainRoot::Full(area, 0)),
      None => layout
        .hidden_main
        .map(|(bar, playbar)| MainRoot::Hidden { bar, playbar }),
    },
    None => Some(MainRoot::Full(
      Rect::new(0, 0, app.view.size.width, app.view.size.height),
      main_layout_margin(app),
    )),
  }
}

/// The rect of `pane` in the current layout, when it shows.
pub fn pane_area(app: &App, pane: Pane) -> Option<Rect> {
  compute_pane_layout(app)?.area(pane)
}

/// Whether the current route draws the main layout and with it the panes; keep in step with
/// the draw match in `runner::start_ui`.
pub fn route_draws_main_layout(app: &App) -> bool {
  match app.get_current_route().active_block {
    ActiveBlock::HelpMenu
    | ActiveBlock::Queue
    | ActiveBlock::Error
    | ActiveBlock::SelectDevice
    | ActiveBlock::Analysis
    | ActiveBlock::LyricsView
    | ActiveBlock::MiniPlayer
    | ActiveBlock::AnnouncementPrompt
    | ActiveBlock::ExitPrompt
    | ActiveBlock::Settings
    | ActiveBlock::PluginScreen => false,
    ActiveBlock::CoverArtView => !cfg!(feature = "cover-art"),
    _ => true,
  }
}

/// The rect of `pane` when it is on screen in this frame.
pub fn visible_pane_area(app: &App, pane: Pane) -> Option<Rect> {
  if !route_draws_main_layout(app) {
    return None;
  }
  pane_area(app, pane)
}

/// Whether the panes are on screen and can take keys: no full-screen view or modal covers them.
pub fn main_layout_has_keys(app: &App) -> bool {
  !matches!(
    app.get_current_route().active_block,
    ActiveBlock::HelpMenu
      | ActiveBlock::Queue
      | ActiveBlock::Party
      | ActiveBlock::Error
      | ActiveBlock::SelectDevice
      | ActiveBlock::Analysis
      | ActiveBlock::LyricsView
      | ActiveBlock::MiniPlayer
      | ActiveBlock::CoverArtView
      | ActiveBlock::AnnouncementPrompt
      | ActiveBlock::RecapPrompt
      | ActiveBlock::CommunityPinPrompt
      | ActiveBlock::ExitPrompt
      | ActiveBlock::Settings
      | ActiveBlock::PluginScreen
      | ActiveBlock::CreatePlaylistForm
      | ActiveBlock::Dialog(_)
      | ActiveBlock::SortMenu
      | ActiveBlock::Input
  )
}

/// The detached pane that gets the keys, when one has focus and shows.
pub fn focused_detached_pane(app: &App) -> Option<Pane> {
  let pane = app.view.focused_pane;
  if pane == Pane::Main || !main_layout_has_keys(app) {
    return None;
  }
  pane_area(app, pane).map(|_| pane)
}

/// Where the close mark sits on a pane's top border, when the pane is wide enough.
pub fn pane_close_mark(area: Rect) -> Option<Rect> {
  const MARK_WIDTH: u16 = 3;
  (area.width >= MARK_WIDTH + 4 && area.height > 0).then(|| Rect {
    x: area.x + area.width - MARK_WIDTH - 2,
    y: area.y,
    width: MARK_WIDTH,
    height: 1,
  })
}

/// The split axis for a new pane beside `area`: side by side when the area is
/// wide, counting a cell as about twice as tall as it is wide.
pub fn detach_axis(area: Rect) -> SplitAxis {
  if area.width >= area.height.saturating_mul(2) {
    SplitAxis::Horizontal
  } else {
    SplitAxis::Vertical
  }
}

/// The nearest pane in `direction` from `from` that shares a row or column with it.
pub fn neighbor_pane(layout: &PaneLayout, from: Pane, direction: Arrow) -> Option<Pane> {
  let origin = layout.area(from)?;
  let overlap = |a_start: u16, a_len: u16, b_start: u16, b_len: u16| {
    let start = a_start.max(b_start);
    let end = (a_start + a_len).min(b_start + b_len);
    end.saturating_sub(start)
  };
  layout
    .panes
    .iter()
    .filter(|(pane, _)| *pane != from)
    .filter_map(|(pane, area)| {
      let (distance, shared) = match direction {
        Arrow::Left if area.x + area.width <= origin.x => (
          origin.x - (area.x + area.width),
          overlap(origin.y, origin.height, area.y, area.height),
        ),
        Arrow::Right if area.x >= origin.x + origin.width => (
          area.x - (origin.x + origin.width),
          overlap(origin.y, origin.height, area.y, area.height),
        ),
        Arrow::Up if area.y + area.height <= origin.y => (
          origin.y - (area.y + area.height),
          overlap(origin.x, origin.width, area.x, area.width),
        ),
        Arrow::Down if area.y >= origin.y + origin.height => (
          area.y - (origin.y + origin.height),
          overlap(origin.x, origin.width, area.x, area.width),
        ),
        _ => return None,
      };
      (shared > 0).then_some((distance, std::cmp::Reverse(shared), *pane))
    })
    .min_by_key(|(distance, shared, _)| (*distance, *shared))
    .map(|(_, _, pane)| pane)
}

pub fn compute_main_layout(app: &App) -> Option<MainLayoutAreas> {
  let (root, margin) = match main_root(app)? {
    MainRoot::Full(root, margin) => (root, margin),
    MainRoot::Hidden { bar, playbar } => {
      let [input, help, settings] = split_input_help_and_settings(app, bar);
      return Some(MainLayoutAreas {
        input: Some(input),
        help: Some(help),
        settings: Some(settings),
        playbar,
        ..MainLayoutAreas::default()
      });
    }
  };
  let wide_layout = is_wide_width(app, root.width);
  let behavior = &app.user_config.behavior;
  let runtime_state = &app.runtime_state;

  let (input_area, routes_area, playbar_area) = if wide_layout {
    if behavior.playbar_position == "top" {
      let [playbar_area, routes_area] = root.layout(
        &Layout::vertical([playbar_constraint(runtime_state), Constraint::Min(1)]).margin(margin),
      );
      (None, routes_area, playbar_area)
    } else {
      let [routes_area, playbar_area] = root.layout(
        &Layout::vertical([Constraint::Min(1), playbar_constraint(runtime_state)]).margin(margin),
      );
      (None, routes_area, playbar_area)
    }
  } else if behavior.playbar_position == "top" {
    let [playbar_area, input_area, routes_area] = root.layout(
      &Layout::vertical([
        playbar_constraint(runtime_state),
        Constraint::Length(3),
        Constraint::Min(1),
      ])
      .margin(margin),
    );
    (Some(input_area), routes_area, playbar_area)
  } else {
    let [input_area, routes_area, playbar_area] = root.layout(
      &Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(1),
        playbar_constraint(runtime_state),
      ])
      .margin(margin),
    );
    (Some(input_area), routes_area, playbar_area)
  };

  let (user_area, content_area) = split_routes_area(app, routes_area);
  let library = library_constraints(runtime_state);
  let (input, help, settings, library_area, playlist_area) = if wide_layout {
    let [input_area, library_area, playlist_area] = user_area.layout(&Layout::vertical([
      Constraint::Length(3),
      library[0],
      library[1],
    ]));
    let [input_text_area, help_area, settings_area] =
      split_input_help_and_settings(app, input_area);
    (
      Some(input_text_area),
      Some(help_area),
      Some(settings_area),
      library_area,
      playlist_area,
    )
  } else {
    let [library_area, playlist_area] =
      user_area.layout(&Layout::vertical([library[0], library[1]]));
    if let Some(input_area) = input_area {
      let [input_text_area, help_area, settings_area] =
        split_input_help_and_settings(app, input_area);
      (
        Some(input_text_area),
        Some(help_area),
        Some(settings_area),
        library_area,
        playlist_area,
      )
    } else {
      (None, None, None, library_area, playlist_area)
    }
  };

  Some(MainLayoutAreas {
    input,
    help,
    settings,
    playbar: playbar_area,
    sidebar: user_area,
    library: library_area,
    playlists: playlist_area,
    content: content_area,
  })
}

/// Returns horizontal constraints for the [sidebar, content] split based on runtime state.
/// When sidebar_width_percent is 0, the sidebar is hidden (zero length).
/// When sidebar_width_percent is 100, content is hidden.
pub fn sidebar_constraints(state: &RuntimeState) -> [Constraint; 2] {
  let sidebar = state.sidebar_width_percent.min(100) as u16;
  let content = 100u16.saturating_sub(sidebar);
  [
    Constraint::Percentage(sidebar),
    Constraint::Percentage(content),
  ]
}

fn split_routes_area(app: &App, routes_area: Rect) -> (Rect, Rect) {
  let behavior = &app.user_config.behavior;
  let sidebar = if sidebar_is_hidden(app) {
    [Constraint::Length(0), Constraint::Min(1)]
  } else {
    sidebar_constraints(&app.runtime_state)
  };

  match behavior.sidebar_position.as_str() {
    "right" => {
      let [content_area, user_area] =
        routes_area.layout(&Layout::horizontal([sidebar[1], sidebar[0]]));
      (user_area, content_area)
    }
    _ => {
      let [user_area, content_area] =
        routes_area.layout(&Layout::horizontal([sidebar[0], sidebar[1]]));
      (user_area, content_area)
    }
  }
}

fn sidebar_is_hidden(app: &App) -> bool {
  if app.user_config.behavior.sidebar_position != "hidden" {
    return false;
  }

  let route = app.get_current_route();
  !matches!(
    route.active_block,
    ActiveBlock::Library | ActiveBlock::MyPlaylists
  ) && !matches!(
    route.hovered_block,
    ActiveBlock::Library | ActiveBlock::MyPlaylists
  )
}

pub fn split_input_help_and_settings(app: &App, input_row_area: Rect) -> [Rect; 3] {
  let compact_top_row = input_row_area.width < COMPACT_TOP_ROW_THRESHOLD;

  let constraints = if compact_top_row {
    [
      Constraint::Min(1),
      Constraint::Length(COMPACT_HELP_WIDTH),
      Constraint::Length(COMPACT_SETTINGS_WIDTH),
    ]
  } else if is_wide_layout(app) {
    [
      Constraint::Percentage(65),
      Constraint::Percentage(18),
      Constraint::Percentage(17),
    ]
  } else {
    [
      Constraint::Percentage(80),
      Constraint::Percentage(8),
      Constraint::Percentage(12),
    ]
  };

  input_row_area.layout(&Layout::horizontal(constraints))
}

/// Returns the playbar height constraint based on runtime state.
/// When playbar_height_rows is 0, the playbar is hidden.
pub fn playbar_constraint(state: &RuntimeState) -> Constraint {
  Constraint::Length(state.playbar_height_rows)
}

/// Returns vertical constraints for the [library, playlists] split within the sidebar.
pub fn library_constraints(state: &RuntimeState) -> [Constraint; 2] {
  let library = state.library_height_percent.min(100) as u16;
  let playlists = 100u16.saturating_sub(library);
  [
    Constraint::Percentage(library),
    Constraint::Percentage(playlists),
  ]
}

/// Returns the fullscreen content/playbar split used by lyrics and cover-art views.
///
/// When `playbar_height_rows` is 0, the playbar is hidden and the content area fills the screen.
pub fn fullscreen_view_layout(state: &RuntimeState, area: Rect) -> (Rect, Option<Rect>) {
  if state.playbar_height_rows == 0 {
    return (area, None);
  }

  let chunks = Layout::vertical([
    Constraint::Min(0),
    Constraint::Length(state.playbar_height_rows),
  ])
  .split(area);
  let content_area = chunks[0];
  let playbar_area = chunks[1];

  (content_area, Some(playbar_area))
}

/// Centers a `size` rect inside `bounds`, clamping it to fit.
///
/// Shared by every cover-art placement (playbar slot, fullscreen view, plugin
/// screen widget): `ratatui-image` renders top-left within the area it is
/// given, so the caller measures the fitted size first and then centers it.
#[cfg(feature = "cover-art")]
pub fn center_rect_within(bounds: Rect, size: Rect) -> Rect {
  Rect {
    x: bounds.x + bounds.width.saturating_sub(size.width.min(bounds.width)) / 2,
    y: bounds.y + bounds.height.saturating_sub(size.height.min(bounds.height)) / 2,
    width: size.width.min(bounds.width),
    height: size.height.min(bounds.height),
  }
}

/// Returns the compact playbar area used by the full-screen miniplayer.
///
/// The normal playbar layout is optimized for a short strip. Miniplayer keeps
/// that same playbar renderer, but constrains it to a centered stage so wide
/// terminals do not stretch track metadata across a mostly empty full screen.
pub fn miniplayer_playbar_area(area: Rect) -> Rect {
  let horizontal_margin = if area.width >= 100 { 4 } else { 1 };
  let vertical_margin = if area.height >= 30 { 4 } else { 1 };

  let available_width = area.width.saturating_sub(horizontal_margin * 2);
  let available_height = area.height.saturating_sub(vertical_margin * 2);
  let width = available_width.min(120).max(available_width.min(1));
  let height = available_height.min(18).max(available_height.min(1));

  Rect {
    x: area.x.saturating_add(area.width.saturating_sub(width) / 2),
    y: area
      .y
      .saturating_add(area.height.saturating_sub(height) / 2),
    width,
    height,
  }
}

/// Main on the left, Lyrics above the Visualizer on the right.
#[cfg(test)]
pub(crate) fn app_with_panes(width: u16, height: u16) -> App {
  let mut app = App::default();
  app.view.size = crate::core::geometry::Viewport { width, height };
  app.change_pane_layout(|tree| tree.detach(Pane::Main, Pane::Lyrics, SplitAxis::Horizontal));
  app.change_pane_layout(|tree| tree.detach(Pane::Lyrics, Pane::Visualizer, SplitAxis::Vertical));
  app
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn the_main_layout_fills_only_the_main_pane() {
    let app = app_with_panes(160, 50);
    let main = pane_area(&app, Pane::Main).unwrap();
    let lyrics = pane_area(&app, Pane::Lyrics).unwrap();
    let visualizer = pane_area(&app, Pane::Visualizer).unwrap();

    let areas = compute_main_layout(&app).unwrap();

    assert_eq!(main, Rect::new(1, 1, 95, 48));
    assert_eq!(areas.playbar.x + areas.playbar.width, main.x + main.width);
    assert_eq!(lyrics.x, main.x + main.width);
    assert_eq!(visualizer.y, lyrics.y + lyrics.height);
  }

  #[test]
  fn a_hidden_main_leaves_its_search_row_above_and_its_playbar_below_the_panes() {
    let mut app = app_with_panes(160, 50);
    app.change_pane_layout(|tree| tree.close(Pane::Main));

    let areas = compute_main_layout(&app).unwrap();
    let input = areas.input.unwrap();
    let lyrics = pane_area(&app, Pane::Lyrics).unwrap();
    let visualizer = pane_area(&app, Pane::Visualizer).unwrap();

    assert!(main_hidden(&app));
    assert!(!is_wide_layout(&app));
    assert_eq!((input.x, input.y + input.height), (lyrics.x, lyrics.y));
    assert_eq!(
      (areas.playbar.x, areas.playbar.y, areas.playbar.width),
      (input.x, visualizer.y + visualizer.height, 158)
    );
    assert_eq!(areas.playbar.height, 6);
    assert_eq!(areas.content, Rect::default());
  }

  #[test]
  fn the_panes_hide_and_main_takes_the_terminal_when_a_pane_would_not_fit() {
    let app = app_with_panes(60, 20);

    assert_eq!(compute_pane_layout(&app), None);
    assert_eq!(compute_main_layout(&app).unwrap().playbar.width, 60);
    assert!(app.pane_open(Pane::Lyrics));
  }

  #[test]
  fn neighbor_pane_follows_the_geometry() {
    let app = app_with_panes(160, 50);
    let layout = compute_pane_layout(&app).unwrap();

    assert_eq!(
      neighbor_pane(&layout, Pane::Main, Arrow::Right),
      Some(Pane::Lyrics)
    );
    assert_eq!(
      neighbor_pane(&layout, Pane::Visualizer, Arrow::Up),
      Some(Pane::Lyrics)
    );
    assert_eq!(
      neighbor_pane(&layout, Pane::Visualizer, Arrow::Left),
      Some(Pane::Main)
    );
    assert_eq!(neighbor_pane(&layout, Pane::Main, Arrow::Left), None);
  }

  #[test]
  fn a_split_border_is_hit_on_both_sides_and_gives_the_share_at_a_cell() {
    let app = app_with_panes(160, 50);
    let layout = compute_pane_layout(&app).unwrap();
    let root = layout.borders[0];
    let edge = root.first.x + root.first.width;

    assert_eq!(root.index, 0);
    assert!(root.is_hit(edge - 1, 10));
    assert!(root.is_hit(edge, 10));
    assert!(!root.is_hit(edge + 1, 10));
    assert_eq!(root.percent_at(root.parent.x + 78, 10), 50);
  }

  #[test]
  fn a_wide_area_detaches_side_by_side_and_a_tall_one_stacks() {
    assert_eq!(detach_axis(Rect::new(0, 0, 100, 40)), SplitAxis::Horizontal);
    assert_eq!(detach_axis(Rect::new(0, 0, 60, 40)), SplitAxis::Vertical);
  }

  fn make_state_with(sidebar_pct: u8, playbar_rows: u16) -> RuntimeState {
    RuntimeState {
      sidebar_width_percent: sidebar_pct,
      playbar_height_rows: playbar_rows,
      ..RuntimeState::default()
    }
  }

  fn make_state_with_library(library_pct: u8) -> RuntimeState {
    RuntimeState {
      library_height_percent: library_pct,
      ..RuntimeState::default()
    }
  }

  fn make_state(sidebar_pct: u8) -> RuntimeState {
    make_state_with(sidebar_pct, 6)
  }

  #[test]
  fn default_sidebar_is_20_percent() {
    let b = make_state(20);
    let [sidebar, content] = sidebar_constraints(&b);
    assert_eq!(sidebar, Constraint::Percentage(20));
    assert_eq!(content, Constraint::Percentage(80));
  }

  #[test]
  fn hidden_sidebar_gives_zero_percent() {
    let b = make_state(0);
    let [sidebar, content] = sidebar_constraints(&b);
    assert_eq!(sidebar, Constraint::Percentage(0));
    assert_eq!(content, Constraint::Percentage(100));
  }

  #[test]
  fn full_sidebar_hides_content() {
    let b = make_state(100);
    let [sidebar, content] = sidebar_constraints(&b);
    assert_eq!(sidebar, Constraint::Percentage(100));
    assert_eq!(content, Constraint::Percentage(0));
  }

  #[test]
  fn over_100_percent_is_clamped() {
    let mut b = make_state(20);
    b.sidebar_width_percent = 255;
    let [sidebar, content] = sidebar_constraints(&b);
    assert_eq!(sidebar, Constraint::Percentage(100));
    assert_eq!(content, Constraint::Percentage(0));
  }

  #[test]
  fn default_playbar_is_6_rows() {
    let b = make_state_with(20, 6);
    assert_eq!(playbar_constraint(&b), Constraint::Length(6));
  }

  #[test]
  fn hidden_playbar_is_zero_rows() {
    let b = make_state_with(20, 0);
    assert_eq!(playbar_constraint(&b), Constraint::Length(0));
  }

  #[test]
  fn default_library_is_30_percent() {
    let b = make_state_with_library(30);
    let [lib, playlists] = library_constraints(&b);
    assert_eq!(lib, Constraint::Percentage(30));
    assert_eq!(playlists, Constraint::Percentage(70));
  }

  #[test]
  fn hidden_library_gives_zero_percent() {
    let b = make_state_with_library(0);
    let [lib, playlists] = library_constraints(&b);
    assert_eq!(lib, Constraint::Percentage(0));
    assert_eq!(playlists, Constraint::Percentage(100));
  }

  #[test]
  fn library_over_100_percent_is_clamped() {
    let mut b = make_state_with_library(30);
    b.library_height_percent = 255;
    let [lib, playlists] = library_constraints(&b);
    assert_eq!(lib, Constraint::Percentage(100));
    assert_eq!(playlists, Constraint::Percentage(0));
  }

  #[test]
  fn fullscreen_layout_hides_playbar_when_height_is_zero() {
    let b = make_state_with(20, 0);
    let area = Rect::new(2, 4, 80, 24);

    let (content, playbar) = fullscreen_view_layout(&b, area);

    assert_eq!(content, area);
    assert!(playbar.is_none());
  }

  #[test]
  fn fullscreen_layout_splits_content_and_playbar_when_height_is_set() {
    let b = make_state_with(20, 6);
    let area = Rect::new(2, 4, 80, 24);

    let (content, playbar) = fullscreen_view_layout(&b, area);

    assert_eq!(content, Rect::new(2, 4, 80, 18));
    assert_eq!(playbar, Some(Rect::new(2, 22, 80, 6)));
  }

  #[test]
  fn miniplayer_playbar_area_is_compact_and_centered_on_large_terminals() {
    let area = Rect::new(0, 0, 180, 50);

    assert_eq!(miniplayer_playbar_area(area), Rect::new(30, 16, 120, 18));
  }

  #[test]
  fn miniplayer_playbar_area_uses_available_space_on_small_terminals() {
    let area = Rect::new(0, 0, 60, 12);

    assert_eq!(miniplayer_playbar_area(area), Rect::new(1, 1, 58, 10));
  }
}
