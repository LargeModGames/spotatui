import type { BarPlacement } from "./bindings/BarPlacement";
import type { FrameBar } from "./bindings/FrameBar";
import type { WindowEdge } from "./bindings/WindowEdge";
import { band, DEFAULT_LAYOUT, vertical, type Rect } from "./paneModel";

export const DEFAULT_BARS: BarPlacement[] = DEFAULT_LAYOUT.bars;

/** The width of a bar on the left or right edge, and its height on the top or bottom. */
const THICKNESS: Record<FrameBar, { across: number; along: number }> = {
  top: { across: 64, along: 64 },
  player: { across: 72, along: 280 },
};

/** The gap between the frame's grid cells; style.css has it too. */
const GAP = 8;

const thickness = (bar: FrameBar, edge: WindowEdge) =>
  vertical(edge) ? THICKNESS[bar].along : THICKNESS[bar].across;

export const edgeOf = (bars: BarPlacement[], bar: FrameBar): WindowEdge =>
  bars.find((placement) => placement.bar === bar)?.edge ?? "top";

/** The frame's CSS grid: each bar in list order takes a whole side of what is left. */
export function frameGrid(bars: BarPlacement[]): {
  areas: string;
  rows: string;
  columns: string;
  padding: string;
  /** How far the bars on the bottom edge reach up, in pixels. */
  insetBottom: number;
} {
  const tracks: Record<WindowEdge, number[]> = {
    top: [],
    bottom: [],
    left: [],
    right: [],
  };
  const spans: {
    name: string;
    edge: WindowEdge;
    before: Record<WindowEdge, number>;
  }[] = [];
  for (const { bar, edge } of bars) {
    spans.push({
      name: bar,
      edge,
      before: {
        top: tracks.top.length,
        bottom: tracks.bottom.length,
        left: tracks.left.length,
        right: tracks.right.length,
      },
    });
    tracks[edge].push(thickness(bar, edge));
  }
  const rows = tracks.top.length + 1 + tracks.bottom.length;
  const columns = tracks.left.length + 1 + tracks.right.length;
  const cells = Array.from({ length: rows }, () =>
    Array.from({ length: columns }, () => "panes"),
  );
  for (const { name, edge, before } of spans) {
    const rowFrom = before.top;
    const rowTo = rows - before.bottom;
    const colFrom = before.left;
    const colTo = columns - before.right;
    const fill = (r0: number, r1: number, c0: number, c1: number) => {
      for (let r = r0; r < r1; r++)
        for (let c = c0; c < c1; c++) cells[r]![c] = name;
    };
    if (edge === "top") fill(rowFrom, rowFrom + 1, colFrom, colTo);
    else if (edge === "bottom") fill(rowTo - 1, rowTo, colFrom, colTo);
    else if (edge === "left") fill(rowFrom, rowTo, colFrom, colFrom + 1);
    else fill(rowFrom, rowTo, colTo - 1, colTo);
  }
  const edgeHas = (edge: WindowEdge) => tracks[edge].length > 0;
  const sizes = (list: number[]) => list.map((size) => `${size}px`);
  const bottom = tracks.bottom;
  return {
    areas: cells.map((row) => `"${row.join(" ")}"`).join(" "),
    rows: [
      ...sizes(tracks.top),
      "minmax(0, 1fr)",
      ...sizes(bottom).reverse(),
    ].join(" "),
    columns: [
      ...sizes(tracks.left),
      "minmax(0, 1fr)",
      ...sizes(tracks.right).reverse(),
    ].join(" "),
    padding: `${edgeHas("top") ? 0 : GAP}px ${GAP}px ${edgeHas("bottom") ? 0 : GAP}px`,
    insetBottom:
      bottom.reduce((sum, size) => sum + size, 0) +
      GAP * Math.max(0, bottom.length - 1),
  };
}

/** The window edge nearest to a point. */
export function edgeAt(
  x: number,
  y: number,
  width: number,
  height: number,
): WindowEdge {
  const distances: [WindowEdge, number][] = [
    ["top", y / height],
    ["bottom", 1 - y / height],
    ["left", x / width],
    ["right", 1 - x / width],
  ];
  return distances.reduce((best, next) => (next[1] < best[1] ? next : best))[0];
}

/** `bar` moved to `edge`, inside any bar already there. */
export function moveBar(
  bars: BarPlacement[],
  bar: FrameBar,
  edge: WindowEdge,
): BarPlacement[] {
  if (edgeOf(bars, bar) === edge) return bars;
  const rest = bars.filter((placement) => placement.bar !== bar);
  return [...rest, { bar, edge }];
}

/** The bar's footprint on `edge` of a window, in pixels, for the drop preview. */
export const barPreview = (
  bar: FrameBar,
  edge: WindowEdge,
  width: number,
  height: number,
): Rect =>
  band({ x: 0, y: 0, w: width, h: height }, edge, thickness(bar, edge));
