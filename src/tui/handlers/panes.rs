//! The pane keys, and the keys of a detached pane while it has focus.

#[cfg(feature = "cover-art")]
use super::cover_art_view;
use super::{analysis, is_input_mode, lyrics_view, miniplayer, queue_menu};
use crate::core::action::{Action, NavTarget};
use crate::core::app::{ActiveBlock, App};
use crate::core::input::Arrow;
use crate::core::pane::{Pane, PaneNode};
use crate::tui::event::Key;
use crate::tui::layout::{
  compute_pane_layout, compute_pane_layout_for, detach_axis, focused_detached_pane,
  main_layout_has_keys, neighbor_pane, pane_area,
};
use ratatui::layout::Rect;

const RESIZE_STEP: i16 = 5;
const STATUS_TTL_SECS: u64 = 4;

/// Handle a key of the pane layout; false when `key` is not one.
pub(super) fn handle_pane_key(key: Key, app: &mut App) -> bool {
  let keys = app.user_config.keys.clone();
  if key == keys.detach_pane {
    match focused_detached_pane(app) {
      Some(pane) => attach_pane(app, pane),
      None => detach_current_view(app),
    }
    return true;
  }
  if !main_layout_has_keys(app) {
    return false;
  }
  if key == keys.reset_panes {
    app.change_pane_layout(|tree| std::mem::take(tree) != PaneNode::default());
    return true;
  }
  if key == Key::Esc && leave_focused_pane(app) {
    return true;
  }
  let open_keys = [
    (keys.lyrics_view, Pane::Lyrics),
    (keys.audio_analysis, Pane::Visualizer),
    (keys.cover_art_view, Pane::CoverArt),
    (keys.show_queue, Pane::Queue),
    (keys.miniplayer_view, Pane::MiniPlayer),
  ];
  if let Some((_, pane)) = open_keys
    .into_iter()
    .find(|(open, pane)| *open == key && pane_area(app, *pane).is_some())
  {
    app.view.focused_pane = pane;
    return true;
  }

  let directions = [
    (keys.focus_pane_left, keys.move_pane_left, Arrow::Left),
    (keys.focus_pane_down, keys.move_pane_down, Arrow::Down),
    (keys.focus_pane_up, keys.move_pane_up, Arrow::Up),
    (keys.focus_pane_right, keys.move_pane_right, Arrow::Right),
  ];
  let op = directions
    .into_iter()
    .find_map(|(focus_key, move_key, arrow)| {
      if key == focus_key || key == Key::AltArrow(arrow) {
        Some(PaneOp::Focus(arrow))
      } else if key == move_key || key == Key::AltShiftArrow(arrow) {
        Some(PaneOp::Swap(arrow))
      } else {
        None
      }
    })
    .or_else(|| {
      if key == keys.close_pane {
        Some(PaneOp::Close)
      } else if key == keys.rotate_pane {
        Some(PaneOp::Rotate)
      } else if key == keys.grow_pane {
        Some(PaneOp::Resize(RESIZE_STEP))
      } else if key == keys.shrink_pane {
        Some(PaneOp::Resize(-RESIZE_STEP))
      } else {
        None
      }
    });
  let Some(op) = op else {
    return false;
  };

  let Some(layout) = compute_pane_layout(app) else {
    if *app.pane_layout() != PaneNode::default() {
      handle_hidden_panes(matches!(op, PaneOp::Close), app);
    }
    return true;
  };
  let focused = focused_detached_pane(app).unwrap_or(Pane::Main);
  match op {
    PaneOp::Focus(arrow) => {
      if let Some(neighbor) = neighbor_pane(&layout, focused, arrow) {
        app.view.focused_pane = neighbor;
      }
    }
    PaneOp::Swap(arrow) => {
      if let Some(neighbor) = neighbor_pane(&layout, focused, arrow) {
        app.change_pane_layout(|tree| tree.swap(focused, neighbor));
      }
    }
    PaneOp::Close => {
      app.change_pane_layout(|tree| tree.close(focused));
    }
    PaneOp::Rotate => {
      app.change_pane_layout(|tree| tree.rotate(focused));
    }
    PaneOp::Resize(step) => {
      app.change_pane_layout(|tree| tree.resize(focused, step));
    }
  }
  true
}

enum PaneOp {
  Focus(Arrow),
  Swap(Arrow),
  Close,
  Rotate,
  Resize(i16),
}

/// The panes are saved but do not fit the terminal: the close key removes the
/// last one, and every pane key says why nothing shows.
fn handle_hidden_panes(close: bool, app: &mut App) {
  if close {
    if let Some(last) = app
      .pane_layout()
      .panes()
      .into_iter()
      .rfind(|pane| *pane != Pane::Main)
    {
      app.change_pane_layout(|tree| tree.close(last));
    }
  }
  if *app.pane_layout() != PaneNode::default() && compute_pane_layout(app).is_none() {
    let key = app.user_config.keys.close_pane.to_string();
    let message = app.strings().status.app.pane_hidden(&key);
    app.set_status_message(message, STATUS_TTL_SECS);
  }
}

/// Show a hidden `Main` again when a screen opened in it since the stack was `depth_before` deep.
pub fn show_main_for_new_screen(app: &mut App, depth_before: usize) {
  if app.navigation_depth() > depth_before && main_layout_has_keys(app) {
    app.change_pane_layout(PaneNode::show_main);
  }
}

/// Whether Alt with an arrow is a pane key now; elsewhere it stays a plain arrow.
pub fn pane_arrows_apply(app: &App) -> bool {
  !is_input_mode(app) && main_layout_has_keys(app) && app.pane_layout().is_split()
}

/// Give a key to the focused detached pane; false when no pane has focus.
pub(super) fn handle_focused_pane_key(key: Key, app: &mut App) -> bool {
  let Some(pane) = focused_detached_pane(app) else {
    return false;
  };
  if key == app.user_config.keys.back {
    leave_focused_pane(app);
    return true;
  }
  match pane {
    Pane::Main => return false,
    Pane::Lyrics => lyrics_view::handler(key, app),
    Pane::Visualizer => analysis::handler(key, app),
    Pane::CoverArt => {
      #[cfg(feature = "cover-art")]
      cover_art_view::handler(key, app);
    }
    Pane::Queue => queue_menu::handler(key, app),
    Pane::MiniPlayer => miniplayer::handler(key, app),
  }
  true
}

/// Back or Esc in a focused pane: lyrics browsing ends first, then the focus returns to Main.
pub fn leave_focused_pane(app: &mut App) -> bool {
  let Some(pane) = focused_detached_pane(app) else {
    return false;
  };
  if pane == Pane::Lyrics && app.view.lyrics_view.manual_index.is_some() {
    app.view.lyrics_view.manual_index = None;
  } else if app.pane_layout().contains(Pane::Main) {
    app.view.focused_pane = Pane::Main;
  } else {
    return false;
  }
  true
}

/// Close `pane` and open its view full screen again.
fn attach_pane(app: &mut App, pane: Pane) {
  app.change_pane_layout(|tree| tree.close(pane));
  match pane {
    Pane::Main => {}
    Pane::Lyrics => {
      app.apply(Action::Navigate(NavTarget::Lyrics));
    }
    Pane::Visualizer => {
      app.apply(Action::Navigate(NavTarget::Analysis));
    }
    Pane::CoverArt => app.push_navigation_stack(
      crate::core::app::RouteId::CoverArtView,
      ActiveBlock::CoverArtView,
    ),
    Pane::Queue => {
      app.apply(Action::Navigate(NavTarget::Queue));
    }
    Pane::MiniPlayer => {
      app.apply(Action::Navigate(NavTarget::MiniPlayer));
    }
  }
}

fn detach_current_view(app: &mut App) {
  let pane = match app.get_current_route().active_block {
    ActiveBlock::LyricsView => Pane::Lyrics,
    ActiveBlock::Analysis => Pane::Visualizer,
    ActiveBlock::CoverArtView => Pane::CoverArt,
    ActiveBlock::Queue => Pane::Queue,
    ActiveBlock::MiniPlayer => Pane::MiniPlayer,
    _ => {
      let key = app.user_config.keys.detach_pane.to_string();
      let message = app.strings().status.app.pane_nothing_to_detach(&key);
      app.set_status_message(message, STATUS_TTL_SECS);
      return;
    }
  };
  if app.pane_open(pane) {
    handle_hidden_panes(false, app);
    return;
  }

  let focused = app.view.focused_pane;
  let (target, area) = pane_area(app, focused)
    .map(|area| (focused, area))
    .unwrap_or((
      Pane::Main,
      Rect::new(0, 0, app.view.size.width, app.view.size.height),
    ));
  let mut tree = app.pane_layout().clone();
  if !tree.detach(target, pane, detach_axis(area)) || compute_pane_layout_for(app, &tree).is_none()
  {
    let message = app.strings().status.app.pane_no_room;
    app.set_status_message(message, STATUS_TTL_SECS);
    return;
  }
  app.pop_navigation_stack();
  app.change_pane_layout(|saved| {
    *saved = tree;
    true
  });
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::app::RouteId;
  use crate::core::user_config::VisualizerStyle;
  use crate::tui::handlers::handle_app;
  use crate::tui::layout::app_with_panes;

  #[test]
  fn alt_d_in_the_lyrics_view_detaches_it_beside_main() {
    let mut app = App::default();
    app.view.size = crate::core::geometry::Viewport {
      width: 160,
      height: 50,
    };
    app.push_navigation_stack(RouteId::LyricsView, ActiveBlock::LyricsView);

    handle_app(Key::Alt('d'), &mut app);

    assert_ne!(app.get_current_route().id, RouteId::LyricsView);
    assert_eq!(app.pane_layout().panes(), vec![Pane::Main, Pane::Lyrics]);
    assert_eq!(app.view.focused_pane, Pane::Main);
  }

  #[test]
  fn alt_d_without_a_full_screen_view_explains_itself_and_changes_nothing() {
    let mut app = App::default();

    handle_app(Key::Alt('d'), &mut app);

    assert!(!app.pane_layout().is_split());
    assert!(app.status_message().is_some_and(|m| m.contains("<Alt+d>")));
  }

  #[test]
  fn the_open_key_of_a_detached_view_focuses_its_pane() {
    let mut app = app_with_panes(160, 50);

    handle_app(app.user_config.keys.lyrics_view, &mut app);

    assert_eq!(app.view.focused_pane, Pane::Lyrics);
    assert_ne!(app.get_current_route().id, RouteId::LyricsView);
  }

  #[test]
  fn a_key_in_the_focused_visualizer_pane_reaches_the_visualizer() {
    let mut app = app_with_panes(160, 50);
    app.view.focused_pane = Pane::Visualizer;
    let before = app.user_config.behavior.visualizer_style;

    handle_app(Key::Char('V'), &mut app);

    let after: VisualizerStyle = app.user_config.behavior.visualizer_style;
    assert_ne!(after, before);
    assert_eq!(app.view.focused_pane, Pane::Visualizer);
  }

  #[test]
  fn esc_in_a_focused_pane_gives_the_focus_back_to_main() {
    let mut app = app_with_panes(160, 50);
    app.view.focused_pane = Pane::Visualizer;
    let route = app.get_current_route().id.clone();

    handle_app(Key::Esc, &mut app);

    assert_eq!(app.view.focused_pane, Pane::Main);
    assert_eq!(app.get_current_route().id, route);
  }

  #[test]
  fn focus_keys_move_between_panes_and_alt_x_closes_the_focused_one() {
    let mut app = app_with_panes(160, 50);

    handle_app(Key::Alt('l'), &mut app);
    assert_eq!(app.view.focused_pane, Pane::Lyrics);
    handle_app(Key::Alt('j'), &mut app);
    assert_eq!(app.view.focused_pane, Pane::Visualizer);

    handle_app(Key::Alt('x'), &mut app);

    assert_eq!(app.pane_layout().panes(), vec![Pane::Main, Pane::Lyrics]);
    assert_eq!(app.view.focused_pane, Pane::Main);
  }

  #[test]
  fn move_keys_swap_the_focused_pane_with_its_neighbor() {
    let mut app = app_with_panes(160, 50);
    app.view.focused_pane = Pane::Visualizer;

    handle_app(Key::Alt('K'), &mut app);

    assert_eq!(
      app.pane_layout().panes(),
      vec![Pane::Main, Pane::Visualizer, Pane::Lyrics]
    );
    assert_eq!(app.view.focused_pane, Pane::Visualizer);
  }

  #[test]
  fn pane_keys_wait_while_a_full_screen_view_covers_the_panes() {
    let mut app = app_with_panes(160, 50);
    app.push_navigation_stack(RouteId::HelpMenu, ActiveBlock::HelpMenu);

    handle_app(Key::Alt('x'), &mut app);
    handle_app(Key::Alt('l'), &mut app);

    assert!(app.pane_open(Pane::Visualizer));
    assert_eq!(app.view.focused_pane, Pane::Main);
  }

  #[test]
  fn alt_d_in_a_focused_pane_puts_it_back_full_screen() {
    let mut app = app_with_panes(160, 50);
    app.view.focused_pane = Pane::Lyrics;

    handle_app(Key::Alt('d'), &mut app);

    assert_eq!(app.get_current_route().id, RouteId::LyricsView);
    assert!(!app.pane_open(Pane::Lyrics));
    assert_eq!(app.view.focused_pane, Pane::Main);
  }

  #[test]
  fn a_detach_that_would_not_fit_is_refused_and_keeps_the_view() {
    let mut app = app_with_panes(100, 30);
    let before = app.pane_layout().clone();
    app.push_navigation_stack(RouteId::Queue, ActiveBlock::Queue);

    handle_app(Key::Alt('d'), &mut app);

    assert_eq!(app.pane_layout(), &before);
    assert_eq!(app.get_current_route().id, RouteId::Queue);
    assert_eq!(
      app.status_message(),
      Some(app.strings().status.app.pane_no_room)
    );
  }

  #[test]
  fn alt_x_closes_hidden_panes_one_at_a_time_and_says_why_they_hide() {
    let mut app = app_with_panes(60, 20);

    handle_app(Key::Alt('x'), &mut app);

    assert_eq!(app.pane_layout().panes(), vec![Pane::Main, Pane::Lyrics]);
    assert!(app.status_message().is_some_and(|m| m.contains("<Alt+x>")));

    handle_app(Key::Alt('x'), &mut app);

    assert!(!app.pane_layout().is_split());
  }

  #[test]
  fn alt_0_closes_every_pane_also_when_they_hide() {
    for (width, height) in [(160, 50), (60, 20)] {
      let mut app = app_with_panes(width, height);
      app.view.focused_pane = Pane::Lyrics;

      handle_app(Key::Alt('0'), &mut app);

      assert_eq!(app.pane_layout(), &PaneNode::default());
      assert_eq!(app.view.focused_pane, Pane::Main);
    }
  }

  #[test]
  fn alt_0_brings_main_back_when_one_pane_is_left() {
    let mut app = app_with_panes(160, 50);
    app.change_pane_layout(|tree| {
      *tree = PaneNode::Leaf(Pane::Lyrics);
      true
    });
    app.view.focused_pane = Pane::Lyrics;

    handle_app(Key::Alt('0'), &mut app);

    assert_eq!(app.pane_layout(), &PaneNode::default());
    assert_eq!(app.view.focused_pane, Pane::Main);
  }

  #[test]
  fn alt_x_closes_the_last_hidden_pane_and_brings_main_back() {
    let mut app = app_with_panes(20, 8);
    app.change_pane_layout(|tree| {
      *tree = PaneNode::Leaf(Pane::Lyrics);
      true
    });
    assert!(compute_pane_layout(&app).is_none());

    handle_app(Key::Alt('x'), &mut app);

    assert_eq!(app.pane_layout(), &PaneNode::default());
  }

  #[test]
  fn alt_x_on_main_hides_it_and_alt_0_brings_it_back() {
    let mut app = app_with_panes(160, 50);

    handle_app(Key::Alt('x'), &mut app);

    assert_eq!(
      app.pane_layout().panes(),
      vec![Pane::Lyrics, Pane::Visualizer]
    );
    assert_eq!(app.view.focused_pane, Pane::Lyrics);

    handle_app(Key::Esc, &mut app);
    assert_eq!(app.view.focused_pane, Pane::Lyrics);

    handle_app(Key::Alt('0'), &mut app);

    assert_eq!(app.pane_layout(), &PaneNode::default());
    assert_eq!(app.view.focused_pane, Pane::Main);
  }

  #[test]
  fn a_new_screen_in_main_shows_a_hidden_main_but_a_full_screen_view_does_not() {
    let mut app = app_with_panes(160, 50);
    app.change_pane_layout(|tree| tree.close(Pane::Main));

    let depth = app.navigation_depth();
    app.push_navigation_stack(RouteId::LyricsView, ActiveBlock::LyricsView);
    show_main_for_new_screen(&mut app, depth);
    assert!(!app.pane_open(Pane::Main));

    app.pop_navigation_stack();
    let depth = app.navigation_depth();
    app.push_navigation_stack(RouteId::Search, ActiveBlock::SearchResultBlock);
    show_main_for_new_screen(&mut app, depth);

    assert_eq!(
      app.pane_layout().panes(),
      vec![Pane::Main, Pane::Lyrics, Pane::Visualizer]
    );
  }

  #[test]
  fn alt_arrows_focus_and_alt_shift_arrows_swap() {
    let mut app = app_with_panes(160, 50);

    handle_app(Key::AltArrow(Arrow::Right), &mut app);
    assert_eq!(app.view.focused_pane, Pane::Lyrics);
    handle_app(Key::AltArrow(Arrow::Down), &mut app);
    assert_eq!(app.view.focused_pane, Pane::Visualizer);

    handle_app(Key::AltShiftArrow(Arrow::Up), &mut app);

    assert_eq!(
      app.pane_layout().panes(),
      vec![Pane::Main, Pane::Visualizer, Pane::Lyrics]
    );
  }
}
