import type { GuiLayout } from "./bindings/GuiLayout";
import type { Pane } from "./bindings/Pane";
import {
  band,
  dock,
  dockAtEdge,
  float,
  layoutRects,
  rectsFit,
  swap,
  without,
  type Rect,
  vertical,
  type Side,
} from "./paneModel";

/** A place a dragged pane can land. Boxes are in pixels of the pane area. */
export interface Target {
  id: string;
  zone: Rect;
  result: GuiLayout;
  /** Where the pane will be; null for a drop that changes nothing. */
  preview: Rect | null;
  /** False when the result leaves a pane under its minimum size. */
  ok: boolean;
}

export interface Area {
  w: number;
  h: number;
}

const ROOT_BAND = 24;
const EDGE_BAND = 96;
const EDGE_SHARE = 0.25;
const SWAP_BOX = 0.3;
const SIDES: Side[] = ["left", "right", "top", "bottom"];

const px = (rect: Rect, area: Area): Rect => ({
  x: rect.x * area.w,
  y: rect.y * area.h,
  w: rect.w * area.w,
  h: rect.h * area.h,
});

function centred(box: Rect, part: number): Rect {
  const w = box.w * part;
  const h = box.h * part;
  return { x: box.x + (box.w - w) / 2, y: box.y + (box.h - h) / 2, w, h };
}

/**
 * Every target for dragging `dragged`, in priority order: the area's edges, each other
 * pane's edges, a swap box in each other pane's centre, then the dragged pane's own box.
 * The zones are where the panes are now; the previews are where the dragged pane will be.
 */
export function dockTargets(
  layout: GuiLayout,
  dragged: Pane,
  area: Area,
): Target[] {
  const rest = without(layout, dragged);
  const now = layoutRects(layout.tree).rects;
  const own = now.get(dragged);
  const target = (id: string, zone: Rect, result: GuiLayout): Target => {
    const rects = layoutRects(result.tree).rects;
    const preview = rects.get(dragged);
    return {
      id,
      zone,
      result,
      preview: preview ? px(preview, area) : null,
      ok: rectsFit(rects, area.w, area.h),
    };
  };

  const targets: Target[] = SIDES.map((side) =>
    target(
      `area-${side}`,
      band({ x: 0, y: 0, w: area.w, h: area.h }, side, ROOT_BAND),
      { ...rest, tree: dockAtEdge(rest.tree, dragged, side, 25) },
    ),
  );
  for (const [pane, rect] of now) {
    if (pane === dragged) continue;
    const box = px(rect, area);
    const share = pane === "main" ? 25 : 50;
    for (const side of SIDES) {
      const along = vertical(side) ? box.w : box.h;
      targets.push(
        target(
          `${pane}-${side}`,
          band(box, side, Math.min(along * EDGE_SHARE, EDGE_BAND)),
          { ...rest, tree: dock(rest.tree, dragged, pane, side, share) },
        ),
      );
    }
  }
  if (own) {
    for (const [pane, rect] of now) {
      if (pane === dragged) continue;
      targets.push(
        target(`${pane}-swap`, centred(px(rect, area), SWAP_BOX), {
          ...layout,
          tree: swap(layout.tree, dragged, pane),
        }),
      );
    }
    targets.push({
      id: "own",
      zone: px(own, area),
      result: layout,
      preview: null,
      ok: true,
    });
  }
  return targets;
}

const inside = (rect: Rect, x: number, y: number) =>
  x >= rect.x && x < rect.x + rect.w && y >= rect.y && y < rect.y + rect.h;

/** The first target whose zone holds the point; null means the drop floats the pane. */
export function hitTarget(
  targets: Target[],
  x: number,
  y: number,
): Target | null {
  return targets.find((target) => inside(target.zone, x, y)) ?? null;
}

/** The layout after a drop at the pane box `ghost` (pixels) with no target under it. */
export function floatAt(
  layout: GuiLayout,
  pane: Pane,
  ghost: Rect,
  area: Area,
): GuiLayout {
  const w = Math.min(ghost.w, area.w);
  const h = Math.min(ghost.h, area.h);
  const x = Math.min(Math.max(ghost.x, 0), area.w - w);
  const y = Math.min(Math.max(ghost.y, 0), area.h - h);
  return float(layout, pane, {
    x: x / area.w,
    y: y / area.h,
    w: w / area.w,
    h: h / area.h,
  });
}
