import defaults from "./bindings/default_layout.json";
import type { FloatingPane } from "./bindings/FloatingPane";
import type { GuiLayout } from "./bindings/GuiLayout";
import type { Pane } from "./bindings/Pane";
import type { PaneNode } from "./bindings/PaneNode";
import type { Split } from "./bindings/Split";

/** The Rust `GuiLayout::default()`, written by the bindings test. */
export const DEFAULT_LAYOUT = defaults as unknown as GuiLayout;

export type Side = "left" | "right" | "top" | "bottom";

/** A box in fractions of the pane area, or in pixels where a function says so. */
export interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

export interface Divider {
  /** The split's index in pre-order, as `setSplitPercent` takes it. */
  index: number;
  axis: Split["axis"];
  /** The split's own box. */
  box: Rect;
  percent: number;
}

const MIN_PERCENT = 10;
const MAX_PERCENT = 90;
const SNAP_POINTS = [25, 33, 50, 67, 75];
const SNAP_RANGE = 2;
export const MIN_PANE = { w: 240, h: 160 };
export const MIN_MAIN = { w: 560, h: 200 };
const PERMILLE = 1000;

export const isLeaf = (node: PaneNode): node is Pane =>
  typeof node === "string";

export function panes(node: PaneNode): Pane[] {
  return isLeaf(node) ? [node] : [...panes(node.first), ...panes(node.second)];
}

export const contains = (node: PaneNode, pane: Pane): boolean =>
  panes(node).includes(pane);

/** The tree without `pane`; its sibling takes the parent's place, and Main takes a lone pane's. */
export function close(node: PaneNode, pane: Pane): PaneNode {
  if (isLeaf(node)) return node === pane && pane !== "main" ? "main" : node;
  if (node.first === pane) return node.second;
  if (node.second === pane) return node.first;
  const first = close(node.first, pane);
  const second = close(node.second, pane);
  return first === node.first && second === node.second
    ? node
    : { ...node, first, second };
}

const axisOf = (side: Side): Split["axis"] =>
  side === "left" || side === "right" ? "horizontal" : "vertical";

/** `pane` beside `around` on `side`, taking `share` percent. */
function beside(
  around: PaneNode,
  pane: Pane,
  side: Side,
  share: number,
): PaneNode {
  const leading = side === "left" || side === "top";
  return {
    axis: axisOf(side),
    percent: leading ? share : 100 - share,
    first: leading ? pane : around,
    second: leading ? around : pane,
  };
}

/** Split the `target` leaf and put `pane` on its `side`. */
export function dock(
  node: PaneNode,
  pane: Pane,
  target: Pane,
  side: Side,
  share: number,
): PaneNode {
  if (isLeaf(node))
    return node === target ? beside(node, pane, side, share) : node;
  const first = dock(node.first, pane, target, side, share);
  const second = dock(node.second, pane, target, side, share);
  return first === node.first && second === node.second
    ? node
    : { ...node, first, second };
}

/** Put `pane` along a whole edge of the area. */
export const dockAtEdge = (
  node: PaneNode,
  pane: Pane,
  side: Side,
  share: number,
): PaneNode => beside(node, pane, side, share);

/** A hidden Main back on the left, as the Rust `show_main` puts it. */
export const showMain = (node: PaneNode): PaneNode =>
  contains(node, "main") ? node : beside(node, "main", "left", 60);

export function swap(node: PaneNode, a: Pane, b: Pane): PaneNode {
  if (isLeaf(node)) return node === a ? b : node === b ? a : node;
  return {
    ...node,
    first: swap(node.first, a, b),
    second: swap(node.second, a, b),
  };
}

/** Replace the `from` leaf with `to`. */
const replace = (node: PaneNode, from: Pane, to: Pane): PaneNode =>
  swap(node, from, to);

export function setSplitPercent(
  node: PaneNode,
  index: number,
  percent: number,
): PaneNode {
  let seen = 0;
  const walk = (current: PaneNode): PaneNode => {
    if (isLeaf(current)) return current;
    if (seen++ === index) {
      const clamped = clampPercent(percent);
      return clamped === current.percent
        ? current
        : { ...current, percent: clamped };
    }
    const first = walk(current.first);
    const second = walk(current.second);
    return first === current.first && second === current.second
      ? current
      : { ...current, first, second };
  };
  return walk(node);
}

const clampPercent = (percent: number) =>
  Math.min(MAX_PERCENT, Math.max(MIN_PERCENT, Math.round(percent)));

/** A dragged share, pulled to a snap point within a few percent of it. */
export function snapPercent(raw: number): number {
  const near = SNAP_POINTS.find((point) => Math.abs(raw - point) <= SNAP_RANGE);
  return clampPercent(near ?? raw);
}

/** Each leaf's box and each split's divider, in fractions of the area. */
export function layoutRects(node: PaneNode): {
  rects: Map<Pane, Rect>;
  dividers: Divider[];
} {
  const rects = new Map<Pane, Rect>();
  const dividers: Divider[] = [];
  const walk = (current: PaneNode, box: Rect) => {
    if (isLeaf(current)) {
      rects.set(current, box);
      return;
    }
    const index = dividers.length;
    dividers.push({ index, axis: current.axis, box, percent: current.percent });
    const part = current.percent / 100;
    if (current.axis === "horizontal") {
      walk(current.first, { ...box, w: box.w * part });
      walk(current.second, {
        ...box,
        x: box.x + box.w * part,
        w: box.w * (1 - part),
      });
    } else {
      walk(current.first, { ...box, h: box.h * part });
      walk(current.second, {
        ...box,
        y: box.y + box.h * part,
        h: box.h * (1 - part),
      });
    }
  };
  walk(node, { x: 0, y: 0, w: 1, h: 1 });
  return { rects, dividers };
}

/** Whether every leaf gets its minimum size in an area of `width` by `height` pixels. */
export function fits(node: PaneNode, width: number, height: number): boolean {
  for (const [pane, rect] of layoutRects(node).rects) {
    const min = pane === "main" ? MIN_MAIN : MIN_PANE;
    if (rect.w * width < min.w || rect.h * height < min.h) return false;
  }
  return true;
}

export const placed = (layout: GuiLayout, pane: Pane): boolean =>
  contains(layout.tree, pane) ||
  layout.floating.some((float) => float.pane === pane);

/** The layout with `pane` in neither the tree nor the floating list. */
export function without(layout: GuiLayout, pane: Pane): GuiLayout {
  const tree = close(layout.tree, pane);
  const floating = layout.floating.filter((float) => float.pane !== pane);
  return tree === layout.tree && floating.length === layout.floating.length
    ? layout
    : { ...layout, tree, floating };
}

/** `pane` floating on top at `rect`, a box in fractions of the area. */
export function float(layout: GuiLayout, pane: Pane, rect: Rect): GuiLayout {
  const rest = without(layout, pane);
  const permille = (value: number) => Math.round(value * PERMILLE);
  const placement: FloatingPane = {
    pane,
    x: permille(rect.x),
    y: permille(rect.y),
    width: permille(rect.w),
    height: permille(rect.h),
  };
  return { ...rest, floating: [...rest.floating, placement] };
}

/** A floating pane's box in fractions of the area. */
export const floatRect = (float: FloatingPane): Rect => ({
  x: float.x / PERMILLE,
  y: float.y / PERMILLE,
  w: float.width / PERMILLE,
  h: float.height / PERMILLE,
});

/** The side panes take turns in one slot, as the right panel did. */
const SIDE_PARTNER: Partial<Record<Pane, Pane>> = {
  now_playing: "queue",
  queue: "now_playing",
};

/** Open `pane` on the right, or in its partner's slot; close it when it is open. */
export function toggle(layout: GuiLayout, pane: Pane): GuiLayout {
  if (placed(layout, pane)) return without(layout, pane);
  return open(layout, pane);
}

/** `pane` in its partner's slot, or along the right edge; unchanged when it is open. */
export function open(layout: GuiLayout, pane: Pane): GuiLayout {
  if (placed(layout, pane)) return layout;
  const partner = SIDE_PARTNER[pane];
  if (partner && contains(layout.tree, partner))
    return { ...layout, tree: replace(layout.tree, partner, pane) };
  return { ...layout, tree: dockAtEdge(layout.tree, pane, "right", 25) };
}

/** The queue's own close: Now Playing takes its slot back when it is not open elsewhere. */
export function closeQueue(layout: GuiLayout): GuiLayout {
  if (contains(layout.tree, "queue") && !placed(layout, "now_playing"))
    return { ...layout, tree: replace(layout.tree, "queue", "now_playing") };
  return without(layout, "queue");
}

/** The pane next to `from` in direction `side`, by the centres of their boxes. */
export function neighbor(
  rects: Map<Pane, Rect>,
  from: Pane,
  side: Side,
): Pane | null {
  const origin = rects.get(from);
  if (!origin) return null;
  const centre = (rect: Rect) => ({
    x: rect.x + rect.w / 2,
    y: rect.y + rect.h / 2,
  });
  const start = centre(origin);
  let best: { pane: Pane; distance: number } | null = null;
  for (const [pane, rect] of rects) {
    if (pane === from) continue;
    const ahead =
      side === "left"
        ? rect.x + rect.w <= origin.x + 1e-6
        : side === "right"
          ? rect.x >= origin.x + origin.w - 1e-6
          : side === "top"
            ? rect.y + rect.h <= origin.y + 1e-6
            : rect.y >= origin.y + origin.h - 1e-6;
    if (!ahead) continue;
    const point = centre(rect);
    const distance = Math.hypot(point.x - start.x, point.y - start.y);
    if (!best || distance < best.distance) best = { pane, distance };
  }
  return best?.pane ?? null;
}
