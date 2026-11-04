//! The GUI's window layout: the pane tree, the panes floating over it, and the edge of each bar.

use serde::{Deserialize, Serialize};

use super::pane::{Pane, PaneNode, Split, SplitAxis};

/// The share of the pane area the default Main tile takes beside Now Playing.
const DEFAULT_MAIN_PERCENT: u8 = 75;
const PERMILLE: u16 = 1000;
const MIN_FLOAT_PERMILLE: u16 = 100;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(test, feature = "gui"), derive(ts_rs::TS))]
pub struct GuiLayout {
  pub tree: PaneNode,
  /// In paint order, the last one on top.
  #[serde(default)]
  pub floating: Vec<FloatingPane>,
  /// For each edge, the outermost bar first.
  #[serde(default)]
  pub bars: Vec<BarPlacement>,
}

/// A pane over the tree, placed in thousandths of the pane area.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(test, feature = "gui"), derive(ts_rs::TS))]
pub struct FloatingPane {
  pub pane: Pane,
  pub x: u16,
  pub y: u16,
  pub width: u16,
  pub height: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(test, feature = "gui"), derive(ts_rs::TS))]
pub struct BarPlacement {
  pub bar: FrameBar,
  pub edge: WindowEdge,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(all(test, feature = "gui"), derive(ts_rs::TS))]
pub enum FrameBar {
  Top,
  Player,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(all(test, feature = "gui"), derive(ts_rs::TS))]
pub enum WindowEdge {
  Top,
  Bottom,
  Left,
  Right,
}

impl FrameBar {
  const ALL: [Self; 2] = [Self::Top, Self::Player];

  fn at_default_edge(self) -> BarPlacement {
    let edge = match self {
      Self::Top => WindowEdge::Top,
      Self::Player => WindowEdge::Bottom,
    };
    BarPlacement { bar: self, edge }
  }
}

fn default_tree() -> PaneNode {
  PaneNode::Split(Box::new(Split {
    axis: SplitAxis::Horizontal,
    percent: DEFAULT_MAIN_PERCENT,
    first: PaneNode::Leaf(Pane::Main),
    second: PaneNode::Leaf(Pane::NowPlaying),
  }))
}

impl Default for GuiLayout {
  fn default() -> Self {
    Self {
      tree: default_tree(),
      floating: Vec::new(),
      bars: FrameBar::ALL.map(FrameBar::at_default_edge).to_vec(),
    }
  }
}

impl GuiLayout {
  /// The layout with each pane in one place and each bar on one edge; Main can be hidden.
  pub fn sanitized(&self) -> Self {
    let mut tree = self.tree.sanitized().unwrap_or_else(default_tree);
    tree.close(Pane::MiniPlayer);

    let mut floating: Vec<FloatingPane> = Vec::new();
    for float in &self.floating {
      let placed = matches!(float.pane, Pane::Main | Pane::MiniPlayer)
        || tree.contains(float.pane)
        || floating.iter().any(|kept| kept.pane == float.pane);
      if !placed {
        floating.push(float.clamped());
      }
    }

    let mut bars: Vec<BarPlacement> = Vec::new();
    for placement in &self.bars {
      if !bars.iter().any(|kept| kept.bar == placement.bar) {
        bars.push(*placement);
      }
    }
    for bar in FrameBar::ALL {
      if !bars.iter().any(|kept| kept.bar == bar) {
        bars.push(bar.at_default_edge());
      }
    }

    Self {
      tree,
      floating,
      bars,
    }
  }
}

impl FloatingPane {
  fn clamped(self) -> Self {
    let width = self.width.clamp(MIN_FLOAT_PERMILLE, PERMILLE);
    let height = self.height.clamp(MIN_FLOAT_PERMILLE, PERMILLE);
    Self {
      pane: self.pane,
      x: self.x.min(PERMILLE - width),
      y: self.y.min(PERMILLE - height),
      width,
      height,
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn float(pane: Pane) -> FloatingPane {
    FloatingPane {
      pane,
      x: 100,
      y: 100,
      width: 300,
      height: 300,
    }
  }

  fn edges(layout: &GuiLayout) -> Vec<(FrameBar, WindowEdge)> {
    layout.bars.iter().map(|p| (p.bar, p.edge)).collect()
  }

  #[test]
  fn the_default_layout_is_main_beside_now_playing_with_the_bars_at_top_and_bottom() {
    let layout = GuiLayout::default();

    assert_eq!(layout.tree.panes(), vec![Pane::Main, Pane::NowPlaying]);
    assert!(layout.floating.is_empty());
    assert_eq!(
      edges(&layout),
      vec![
        (FrameBar::Top, WindowEdge::Top),
        (FrameBar::Player, WindowEdge::Bottom)
      ]
    );
    assert_eq!(layout.sanitized(), layout);
  }

  #[test]
  fn sanitize_keeps_a_hidden_main_hidden() {
    let layout = GuiLayout {
      tree: PaneNode::Leaf(Pane::Lyrics),
      ..GuiLayout::default()
    };

    assert_eq!(layout.sanitized().tree.panes(), vec![Pane::Lyrics]);
  }

  #[test]
  fn sanitize_drops_a_floating_pane_already_docked_or_repeated() {
    let layout = GuiLayout {
      floating: vec![
        float(Pane::NowPlaying),
        float(Pane::Lyrics),
        float(Pane::Lyrics),
        float(Pane::Main),
      ],
      ..GuiLayout::default()
    };

    assert_eq!(layout.sanitized().floating, vec![float(Pane::Lyrics)]);
  }

  #[test]
  fn sanitize_drops_the_mini_player() {
    let mut tree = default_tree();
    tree.detach(Pane::Main, Pane::MiniPlayer, SplitAxis::Vertical);
    let layout = GuiLayout {
      tree,
      floating: vec![float(Pane::MiniPlayer)],
      ..GuiLayout::default()
    };

    let clean = layout.sanitized();

    assert_eq!(clean.tree, default_tree());
    assert!(clean.floating.is_empty());
  }

  #[test]
  fn sanitize_clamps_floating_rects_into_the_area() {
    let layout = GuiLayout {
      floating: vec![FloatingPane {
        pane: Pane::Queue,
        x: 950,
        y: 2000,
        width: 20,
        height: 4000,
      }],
      ..GuiLayout::default()
    };

    assert_eq!(
      layout.sanitized().floating,
      vec![FloatingPane {
        pane: Pane::Queue,
        x: 900,
        y: 0,
        width: 100,
        height: 1000,
      }]
    );
  }

  #[test]
  fn sanitize_restores_a_missing_bar_and_drops_a_repeat() {
    let layout = GuiLayout {
      bars: vec![
        BarPlacement {
          bar: FrameBar::Player,
          edge: WindowEdge::Left,
        },
        BarPlacement {
          bar: FrameBar::Player,
          edge: WindowEdge::Right,
        },
      ],
      ..GuiLayout::default()
    };

    assert_eq!(
      edges(&layout.sanitized()),
      vec![
        (FrameBar::Player, WindowEdge::Left),
        (FrameBar::Top, WindowEdge::Top)
      ]
    );
  }

  #[test]
  fn a_stored_layout_reads_back_as_snake_case_yaml() {
    let yaml = serde_yaml::to_string(&GuiLayout::default()).unwrap();

    assert!(yaml.contains("second: now_playing"), "{yaml}");
    assert!(yaml.contains("bar: player"), "{yaml}");
    assert_eq!(
      serde_yaml::from_str::<GuiLayout>(&yaml).unwrap(),
      GuiLayout::default()
    );
  }
}
