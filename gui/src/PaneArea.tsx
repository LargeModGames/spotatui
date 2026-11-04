import { EN, type GuiStrings } from "./i18n";
import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import type {
  CSSProperties,
  KeyboardEvent,
  PointerEvent,
  ReactNode,
} from "react";
import type { GuiLayout } from "./bindings/GuiLayout";
import type { Pane } from "./bindings/Pane";
import type { PaneNode } from "./bindings/PaneNode";
import { Glyph } from "./Icon";
import {
  fits,
  floatRect,
  layoutRects,
  MAX_PERCENT,
  MIN_PERCENT,
  rectsFit,
  setSplitPercent,
  snapPercent,
  type Divider,
  type Rect,
} from "./paneModel";
import { placeBox, trackDrag, type Point } from "./pointerDrag";
import {
  dockTargets,
  floatAt,
  hitTarget,
  type Area,
  type Target,
} from "./snapModel";
import "./Panes.css";

export interface PaneView {
  title: string;
  node: ReactNode;
  /** The view draws its own `.panel-head`, which then is the grip. */
  ownHead?: boolean;
}

/** Slot order in the DOM never changes, so a moved pane is never remounted. */
const ORDER: Pane[] = [
  "main",
  "now_playing",
  "queue",
  "lyrics",
  "visualizer",
  "cover_art",
];
const GHOST_MAX = { w: 480, h: 320 };
const STEP_PERCENT = 5;

const boxStyle = (rect: Rect): CSSProperties =>
  ({
    "--x": rect.x,
    "--y": rect.y,
    "--w": rect.w,
    "--h": rect.h,
  }) as CSSProperties;

/** The panes beside Main: docked in the split tree or floating over it. */
export function PaneArea({
  s = EN,
  layout,
  wide,
  main,
  views,
  onLayout,
  onShown,
  onClose,
}: {
  s?: GuiStrings;
  layout: GuiLayout;
  /** The window is wide enough for panes beside Main. */
  wide: boolean;
  main: ReactNode;
  views: Partial<Record<Pane, PaneView>>;
  onLayout: (layout: GuiLayout) => void;
  /** Whether the panes beside Main show at the area's size. */
  onShown: (shown: boolean) => void;
  onClose: (pane: Pane) => void;
}) {
  const area = useRef<HTMLDivElement>(null);
  const ghost = useRef<HTMLDivElement>(null);
  const preview = useRef<HTMLDivElement>(null);
  const [size, setSize] = useState<Area>({ w: 0, h: 0 });
  const [live, setLive] = useState<PaneNode | null>(null);
  const tree = live ?? layout.tree;
  const { rects, dividers } = useMemo(() => layoutRects(tree), [tree]);
  const shown = wide && (size.w === 0 || rectsFit(rects, size.w, size.h));

  useLayoutEffect(() => {
    const element = area.current;
    if (!element) return;
    let frame = 0;
    const observer = new ResizeObserver(([entry]) => {
      cancelAnimationFrame(frame);
      frame = requestAnimationFrame(() => {
        const w = Math.round(entry!.contentRect.width);
        const h = Math.round(entry!.contentRect.height);
        setSize((was) => (was.w === w && was.h === h ? was : { w, h }));
      });
    });
    observer.observe(element);
    return () => {
      cancelAnimationFrame(frame);
      observer.disconnect();
    };
  }, []);

  useEffect(() => onShown(shown), [shown, onShown]);

  const showPreview = (target: Target | null) => {
    placeBox(preview.current, target?.preview ?? null);
    if (target?.preview && preview.current)
      preview.current.dataset.refused = String(!target.ok);
  };

  /** Cancels the drag in progress. */
  const stopDrag = useRef<(() => void) | null>(null);
  useEffect(() => () => stopDrag.current?.(), []);

  const startDrag = (pane: Pane) => (event: PointerEvent<HTMLElement>) => {
    const target = event.target as Element;
    if (event.button !== 0 || stopDrag.current) return;
    if (!target.closest(".panel-head")) return;
    if (target.closest("button, a, input, [role=slider]")) return;
    const slot = event.currentTarget;
    const box = slot.getBoundingClientRect();
    const size = {
      w: Math.min(box.width, GHOST_MAX.w),
      h: Math.min(box.height, GHOST_MAX.h),
    };
    const grab = {
      x: ((event.clientX - box.left) * size.w) / box.width,
      y: Math.min(event.clientY - box.top, size.h - 8),
    };
    const before = layout;
    let bounds: DOMRect | null = null;
    let targets: Target[] = [];
    let hit: Target | null = null;
    const hitAt = (at: Point) =>
      bounds ? hitTarget(targets, at.x - bounds.left, at.y - bounds.top) : null;

    stopDrag.current = trackDrag(event, {
      begin: () => {
        bounds = area.current?.getBoundingClientRect() ?? null;
        if (!bounds) return false;
        targets = dockTargets(before, pane, {
          w: bounds.width,
          h: bounds.height,
        });
        slot.dataset.source = "";
        const element = ghost.current;
        if (element) {
          element.querySelector(".pane-ghost-title")!.textContent =
            views[pane]?.title ?? "";
          element.style.width = `${size.w}px`;
          element.style.height = `${size.h}px`;
          element.dataset.show = "true";
        }
        return true;
      },
      paint: (at) => {
        if (ghost.current)
          ghost.current.style.transform = `translate3d(${at.x - grab.x}px, ${at.y - grab.y}px, 0)`;
        const next = hitAt(at);
        if (next?.id !== hit?.id) showPreview(next);
        hit = next;
      },
      end: (drop) => {
        stopDrag.current = null;
        if (!bounds) return;
        delete slot.dataset.source;
        if (ghost.current) ghost.current.dataset.show = "false";
        showPreview(null);
        if (!drop) return;
        const dropped = hitAt(drop);
        if (dropped) {
          if (dropped.preview && dropped.ok && dropped.result !== before)
            onLayout(dropped.result);
          return;
        }
        onLayout(
          floatAt(
            before,
            pane,
            {
              x: drop.x - grab.x - bounds.left,
              y: drop.y - grab.y - bounds.top,
              ...size,
            },
            { w: bounds.width, h: bounds.height },
          ),
        );
      },
    });
  };

  const dividerDrag =
    (divider: Divider) => (event: PointerEvent<HTMLElement>) => {
      const element = area.current;
      if (event.button !== 0 || !element || stopDrag.current) return;
      const box = element.getBoundingClientRect();
      const base = layout.tree;
      const horizontal = divider.axis === "horizontal";
      let next: PaneNode | null = null;
      let last = divider.percent;
      const follow = (at: Point) => {
        const { x, y, w, h } = divider.box;
        const raw = horizontal
          ? ((at.x - box.left - x * box.width) / (w * box.width)) * 100
          : ((at.y - box.top - y * box.height) / (h * box.height)) * 100;
        const percent = snapPercent(raw);
        if (percent === last) return;
        last = percent;
        const candidate = setSplitPercent(base, divider.index, percent);
        if (fits(candidate, box.width, box.height)) {
          next = candidate;
          setLive(candidate);
        }
      };
      stopDrag.current = trackDrag(event, {
        threshold: 0,
        cursor: horizontal ? "col-resize" : "row-resize",
        paint: follow,
        end: (drop) => {
          stopDrag.current = null;
          if (drop) follow(drop);
          setLive(null);
          if (drop && next && next !== base)
            onLayout({ ...layout, tree: next });
        },
      });
    };

  const setPercent = (divider: Divider, percent: number) => {
    const next = setSplitPercent(layout.tree, divider.index, percent);
    if (next !== layout.tree && fits(next, size.w, size.h))
      onLayout({ ...layout, tree: next });
  };
  const dividerKey = (divider: Divider) => (event: KeyboardEvent) => {
    const back = divider.axis === "horizontal" ? "ArrowLeft" : "ArrowUp";
    const ahead = divider.axis === "horizontal" ? "ArrowRight" : "ArrowDown";
    if (event.key === back) setPercent(divider, divider.percent - STEP_PERCENT);
    else if (event.key === ahead)
      setPercent(divider, divider.percent + STEP_PERCENT);
    else if (event.key === "Enter") setPercent(divider, 50);
    else return;
    event.preventDefault();
  };

  const floating = new Map(
    layout.floating.map((entry, index) => [entry.pane, { entry, index }]),
  );
  const full = { x: 0, y: 0, w: 1, h: 1 };
  return (
    <>
      <div ref={area} className="pane-area">
        {ORDER.map((pane) => {
          if (pane === "main")
            return (
              <div
                key={pane}
                className="pane-slot main-slot"
                data-pane={pane}
                hidden={shown && !rects.has(pane)}
                style={boxStyle(shown ? (rects.get(pane) ?? full) : full)}
              >
                {main}
              </div>
            );
          const view = views[pane];
          const docked = rects.get(pane);
          const float = floating.get(pane);
          if (!shown || !view || (!docked && !float)) return null;
          return (
            <div
              key={pane}
              className={float ? "pane-slot floating" : "pane-slot"}
              data-pane={pane}
              tabIndex={-1}
              style={{
                ...boxStyle(float ? floatRect(float.entry) : docked!),
                zIndex: float ? 5 + float.index : undefined,
              }}
              onPointerDown={startDrag(pane)}
            >
              {view.ownHead ? (
                view.node
              ) : (
                <section className="pane-frame" aria-label={view.title}>
                  <header className="panel-head" title={s.drag_pane}>
                    <h2>{view.title}</h2>
                    <button
                      type="button"
                      className="panel-close"
                      aria-label={s.close_pane}
                      onClick={() => onClose(pane)}
                    >
                      <Glyph name="close" />
                    </button>
                  </header>
                  <div className="pane-body">{view.node}</div>
                </section>
              )}
            </div>
          );
        })}
        {shown &&
          dividers.map((divider) => {
            const { x, y, w, h } = divider.box;
            const part = divider.percent / 100;
            const horizontal = divider.axis === "horizontal";
            return (
              <div
                key={divider.index}
                className="pane-divider"
                role="separator"
                aria-orientation={horizontal ? "vertical" : "horizontal"}
                aria-valuenow={divider.percent}
                aria-valuemin={MIN_PERCENT}
                aria-valuemax={MAX_PERCENT}
                tabIndex={0}
                data-axis={divider.axis}
                style={boxStyle(
                  horizontal
                    ? { x: x + w * part, y, w: 0, h }
                    : { x, y: y + h * part, w, h: 0 },
                )}
                onPointerDown={dividerDrag(divider)}
                onDoubleClick={() => setPercent(divider, 50)}
                onKeyDown={dividerKey(divider)}
              />
            );
          })}
        <div ref={preview} className="drop-preview" data-show="false">
          <span>{s.not_enough_room}</span>
        </div>
      </div>
      <div
        ref={ghost}
        className="pane-ghost"
        data-show="false"
        aria-hidden="true"
      >
        <span className="pane-ghost-title" />
      </div>
    </>
  );
}
