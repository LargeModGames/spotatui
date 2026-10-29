//! The pane layout: the main screen plus views detached beside it, as a binary split tree.

use serde::{Deserialize, Serialize};

pub const MIN_SPLIT_PERCENT: u8 = 10;
pub const MAX_SPLIT_PERCENT: u8 = 90;

/// One leaf of the layout. `Main` is the sidebar, content and playbar.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Pane {
  #[default]
  Main,
  Lyrics,
  Visualizer,
  CoverArt,
  Queue,
  MiniPlayer,
}

/// `Horizontal` puts the two children side by side, `Vertical` stacks them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SplitAxis {
  Horizontal,
  Vertical,
}

impl SplitAxis {
  fn flipped(self) -> Self {
    match self {
      Self::Horizontal => Self::Vertical,
      Self::Vertical => Self::Horizontal,
    }
  }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PaneNode {
  Leaf(Pane),
  Split(Box<Split>),
}

/// `percent` is the share of `first` along `axis`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Split {
  pub axis: SplitAxis,
  pub percent: u8,
  pub first: PaneNode,
  pub second: PaneNode,
}

impl Split {
  /// Set the clamped share of `first`; false when it does not change.
  fn set_percent(&mut self, percent: i16) -> bool {
    let percent = clamp_percent(percent);
    let changed = percent != self.percent;
    self.percent = percent;
    changed
  }
}

impl Default for PaneNode {
  fn default() -> Self {
    Self::Leaf(Pane::Main)
  }
}

impl PaneNode {
  pub fn contains(&self, pane: Pane) -> bool {
    match self {
      Self::Leaf(leaf) => *leaf == pane,
      Self::Split(split) => split.first.contains(pane) || split.second.contains(pane),
    }
  }

  /// The leaves in drawing order.
  pub fn panes(&self) -> Vec<Pane> {
    let mut out = Vec::new();
    self.collect_panes(&mut out);
    out
  }

  fn collect_panes(&self, out: &mut Vec<Pane>) {
    match self {
      Self::Leaf(pane) => out.push(*pane),
      Self::Split(split) => {
        split.first.collect_panes(out);
        split.second.collect_panes(out);
      }
    }
  }

  pub fn is_split(&self) -> bool {
    matches!(self, Self::Split(_))
  }

  /// Split the `target` leaf and put `pane` after it. False when `pane` is
  /// already in the tree or `target` is not.
  pub fn detach(&mut self, target: Pane, pane: Pane, axis: SplitAxis) -> bool {
    if pane == Pane::Main || self.contains(pane) {
      return false;
    }
    let Some(leaf) = self.leaf_mut(target) else {
      return false;
    };
    let percent = if target == Pane::Main { 60 } else { 50 };
    *leaf = Self::Split(Box::new(Split {
      axis,
      percent,
      first: Self::Leaf(target),
      second: Self::Leaf(pane),
    }));
    true
  }

  fn leaf_mut(&mut self, target: Pane) -> Option<&mut Self> {
    match self {
      Self::Leaf(pane) if *pane == target => Some(self),
      Self::Leaf(_) => None,
      Self::Split(split) => match split.first.leaf_mut(target) {
        Some(leaf) => Some(leaf),
        None => split.second.leaf_mut(target),
      },
    }
  }

  /// Remove `pane`; its sibling takes the place of the parent split.
  pub fn close(&mut self, pane: Pane) -> bool {
    let Self::Split(split) = self else {
      return false;
    };
    let survivor = if split.first == Self::Leaf(pane) {
      Some(split.second.clone())
    } else if split.second == Self::Leaf(pane) {
      Some(split.first.clone())
    } else {
      None
    };
    match survivor {
      Some(survivor) => {
        *self = survivor;
        true
      }
      None => split.first.close(pane) || split.second.close(pane),
    }
  }

  pub fn swap(&mut self, a: Pane, b: Pane) -> bool {
    if a == b || !self.contains(a) || !self.contains(b) {
      return false;
    }
    self.map_leaves(&|pane| {
      if pane == a {
        b
      } else if pane == b {
        a
      } else {
        pane
      }
    });
    true
  }

  fn map_leaves(&mut self, f: &impl Fn(Pane) -> Pane) {
    match self {
      Self::Leaf(pane) => *pane = f(*pane),
      Self::Split(split) => {
        split.first.map_leaves(f);
        split.second.map_leaves(f);
      }
    }
  }

  /// Flip the axis of the split that holds `pane`.
  pub fn rotate(&mut self, pane: Pane) -> bool {
    match self.parent_mut(pane) {
      Some((split, _)) => {
        split.axis = split.axis.flipped();
        true
      }
      None => false,
    }
  }

  /// Grow `pane` by `delta` percent of its parent split (shrink when negative).
  pub fn resize(&mut self, pane: Pane, delta: i16) -> bool {
    let Some((split, is_first)) = self.parent_mut(pane) else {
      return false;
    };
    let delta = if is_first { delta } else { -delta };
    split.set_percent(i16::from(split.percent) + delta)
  }

  /// The split whose direct child is the `pane` leaf, and whether it is the first child.
  fn parent_mut(&mut self, pane: Pane) -> Option<(&mut Split, bool)> {
    let Self::Split(split) = self else {
      return None;
    };
    if split.first == Self::Leaf(pane) {
      return Some((split, true));
    }
    if split.second == Self::Leaf(pane) {
      return Some((split, false));
    }
    if split.first.contains(pane) {
      split.first.parent_mut(pane)
    } else {
      split.second.parent_mut(pane)
    }
  }

  /// Set the share of the `index`th split in pre-order, as the mouse drags its border.
  pub fn set_split_percent(&mut self, index: usize, percent: u8) -> bool {
    let mut seen = 0;
    self
      .split_at_mut(index, &mut seen)
      .is_some_and(|split| split.set_percent(i16::from(percent)))
  }

  fn split_at_mut(&mut self, index: usize, seen: &mut usize) -> Option<&mut Split> {
    let Self::Split(split) = self else {
      return None;
    };
    if *seen == index {
      return Some(split);
    }
    *seen += 1;
    if let Some(found) = split.first.split_at_mut(index, seen) {
      return Some(found);
    }
    split.second.split_at_mut(index, seen)
  }

  /// Put a hidden `Main` back on the left of every pane.
  pub fn show_main(&mut self) -> bool {
    if self.contains(Pane::Main) {
      return false;
    }
    *self = Self::Split(Box::new(Split {
      axis: SplitAxis::Horizontal,
      percent: 60,
      first: Self::Leaf(Pane::Main),
      second: std::mem::take(self),
    }));
    true
  }

  /// The tree when it holds no pane twice, with each share clamped.
  pub fn sanitized(&self) -> Option<Self> {
    let panes = self.panes();
    let unique = panes
      .iter()
      .enumerate()
      .all(|(i, pane)| !panes[..i].contains(pane));
    if !unique {
      return None;
    }
    let mut tree = self.clone();
    tree.clamp_percents();
    Some(tree)
  }

  fn clamp_percents(&mut self) {
    if let Self::Split(split) = self {
      split.percent = clamp_percent(i16::from(split.percent));
      split.first.clamp_percents();
      split.second.clamp_percents();
    }
  }
}

fn clamp_percent(percent: i16) -> u8 {
  percent.clamp(i16::from(MIN_SPLIT_PERCENT), i16::from(MAX_SPLIT_PERCENT)) as u8
}

#[cfg(test)]
mod tests {
  use super::*;

  fn main_lyrics_visualizer() -> PaneNode {
    let mut tree = PaneNode::default();
    assert!(tree.detach(Pane::Main, Pane::Lyrics, SplitAxis::Horizontal));
    assert!(tree.detach(Pane::Lyrics, Pane::Visualizer, SplitAxis::Vertical));
    tree
  }

  #[test]
  fn detach_splits_the_target_and_puts_the_pane_after_it() {
    let tree = main_lyrics_visualizer();

    assert_eq!(
      tree.panes(),
      vec![Pane::Main, Pane::Lyrics, Pane::Visualizer]
    );
    let PaneNode::Split(root) = &tree else {
      panic!("root is a split")
    };
    assert_eq!(root.axis, SplitAxis::Horizontal);
    assert_eq!(root.percent, 60);
  }

  #[test]
  fn detach_refuses_a_pane_already_in_the_tree_or_main() {
    let mut tree = main_lyrics_visualizer();
    let before = tree.clone();

    assert!(!tree.detach(Pane::Main, Pane::Lyrics, SplitAxis::Vertical));
    assert!(!tree.detach(Pane::Lyrics, Pane::Main, SplitAxis::Vertical));
    assert!(!tree.detach(Pane::Queue, Pane::CoverArt, SplitAxis::Vertical));
    assert_eq!(tree, before);
  }

  #[test]
  fn close_lets_the_sibling_take_the_parent_place() {
    let mut tree = main_lyrics_visualizer();

    assert!(tree.close(Pane::Lyrics));
    assert_eq!(tree.panes(), vec![Pane::Main, Pane::Visualizer]);
    assert!(tree.close(Pane::Visualizer));
    assert_eq!(tree, PaneNode::default());
  }

  #[test]
  fn closing_main_hides_it_and_show_main_puts_it_back_on_the_left() {
    let mut tree = main_lyrics_visualizer();

    assert!(tree.close(Pane::Main));
    assert_eq!(tree.panes(), vec![Pane::Lyrics, Pane::Visualizer]);
    assert!(tree.show_main());
    assert_eq!(
      tree.panes(),
      vec![Pane::Main, Pane::Lyrics, Pane::Visualizer]
    );
    assert!(!tree.show_main());
    assert!(!PaneNode::default().close(Pane::Main));
  }

  #[test]
  fn swap_exchanges_two_leaves() {
    let mut tree = main_lyrics_visualizer();

    assert!(tree.swap(Pane::Main, Pane::Visualizer));
    assert_eq!(
      tree.panes(),
      vec![Pane::Visualizer, Pane::Lyrics, Pane::Main]
    );
    assert!(!tree.swap(Pane::Main, Pane::Queue));
  }

  #[test]
  fn rotate_flips_the_parent_split_axis() {
    let mut tree = main_lyrics_visualizer();

    assert!(tree.rotate(Pane::Visualizer));
    let PaneNode::Split(root) = &tree else {
      panic!("root is a split")
    };
    let PaneNode::Split(inner) = &root.second else {
      panic!("second is a split")
    };
    assert_eq!(inner.axis, SplitAxis::Horizontal);
    assert!(!PaneNode::default().rotate(Pane::Main));
  }

  #[test]
  fn resize_grows_the_pane_side_and_clamps() {
    let mut tree = main_lyrics_visualizer();

    assert!(tree.resize(Pane::Main, 5));
    let PaneNode::Split(root) = &tree else {
      panic!("root is a split")
    };
    assert_eq!(root.percent, 65);

    assert!(tree.resize(Pane::Lyrics, 80));
    let PaneNode::Split(root) = &tree else {
      panic!("root is a split")
    };
    let PaneNode::Split(inner) = &root.second else {
      panic!("second is a split")
    };
    assert_eq!(inner.percent, MAX_SPLIT_PERCENT);
    assert!(!tree.resize(Pane::Lyrics, 5));
  }

  #[test]
  fn set_split_percent_addresses_splits_in_pre_order() {
    let mut tree = main_lyrics_visualizer();

    assert!(tree.set_split_percent(1, 30));
    assert!(tree.set_split_percent(0, 2));
    assert!(!tree.set_split_percent(2, 50));
    let PaneNode::Split(root) = &tree else {
      panic!("root is a split")
    };
    let PaneNode::Split(inner) = &root.second else {
      panic!("second is a split")
    };
    assert_eq!(root.percent, MIN_SPLIT_PERCENT);
    assert_eq!(inner.percent, 30);
  }

  #[test]
  fn sanitized_keeps_a_hidden_main_and_rejects_a_duplicate() {
    let no_main: PaneNode =
      serde_yaml::from_str("axis: vertical\npercent: 50\nfirst: lyrics\nsecond: queue\n").unwrap();
    let duplicate: PaneNode = serde_yaml::from_str(
      "axis: vertical\npercent: 50\nfirst: main\nsecond:\n  axis: vertical\n  percent: 50\n  first: queue\n  second: queue\n",
    )
    .unwrap();

    assert_eq!(no_main.sanitized(), Some(no_main));
    assert_eq!(duplicate.sanitized(), None);
  }

  #[test]
  fn the_layout_round_trips_as_hand_editable_yaml() {
    let tree = main_lyrics_visualizer();

    let yaml = serde_yaml::to_string(&tree).unwrap();
    assert!(yaml.contains("first: main"), "{yaml}");
    assert!(yaml.contains("first: lyrics"), "{yaml}");
    assert_eq!(serde_yaml::from_str::<PaneNode>(&yaml).unwrap(), tree);
  }
}
