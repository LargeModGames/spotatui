//! Draws the views detached beside `Main` (see `core::pane`).

use crate::core::app::App;
use crate::core::pane::Pane;
use crate::tui::layout::{compute_pane_layout, pane_close_mark};
use ratatui::{
  layout::Rect,
  style::Style,
  widgets::{Block, Borders},
  Frame,
};

pub const CLOSE_MARK: &str = "[x]";

pub fn draw_detached_panes(f: &mut Frame<'_>, app: &App) {
  let Some(layout) = compute_pane_layout(app) else {
    return;
  };
  for (pane, area) in layout.panes {
    if pane == Pane::Main {
      continue;
    }
    draw_pane(f, app, pane, area);
    frame_pane(f, app, area, app.view.focused_pane == pane);
  }
}

fn draw_pane(f: &mut Frame<'_>, app: &App, pane: Pane, area: Rect) {
  match pane {
    Pane::Main => {}
    Pane::Lyrics => super::lyrics::draw_lyrics(f, app, area),
    Pane::Visualizer => super::audio_analysis::draw_in(f, app, area, 0),
    Pane::CoverArt => {
      let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(app.user_config.theme.inactive.into()));
      let inner = block.inner(area);
      f.render_widget(block, area);
      #[cfg(feature = "cover-art")]
      super::player::draw_cover_art_content(f, app, inner);
      #[cfg(not(feature = "cover-art"))]
      let _ = inner;
    }
    Pane::Queue => super::popups::draw_queue_in(f, app, area),
    Pane::MiniPlayer => {
      #[cfg(feature = "cover-art")]
      crate::tui::cover_art::in_pane_playbar(|| super::draw_playbar(f, app, area));
      #[cfg(not(feature = "cover-art"))]
      super::draw_playbar(f, app, area);
    }
  }
}

/// Paint the border cells on the pane's edge in the focus color and put the close mark on it.
fn frame_pane(f: &mut Frame<'_>, app: &App, area: Rect, focused: bool) {
  let theme = &app.user_config.theme;
  let buf = f.buffer_mut();
  if focused {
    let color = theme.active.into();
    let right = area.x + area.width.saturating_sub(1);
    let bottom = area.y + area.height.saturating_sub(1);
    let edge = (area.x..=right)
      .flat_map(|x| [(x, area.y), (x, bottom)])
      .chain((area.y..=bottom).flat_map(|y| [(area.x, y), (right, y)]));
    for position in edge {
      if let Some(cell) = buf.cell_mut(position) {
        if cell.symbol().chars().all(is_box_drawing) {
          cell.set_fg(color);
        }
      }
    }
  }
  if let Some(mark) = pane_close_mark(area) {
    let color = if focused {
      theme.active
    } else {
      theme.inactive
    };
    buf.set_string(
      mark.x,
      mark.y,
      CLOSE_MARK,
      Style::default().fg(color.into()),
    );
  }
}

fn is_box_drawing(c: char) -> bool {
  ('\u{2500}'..='\u{257F}').contains(&c)
}
