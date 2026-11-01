import { EN, type GuiStrings } from "./i18n";
import {
  memo,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from "react";
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
  setSplitPercent,
  snapPercent,
  type Divider,
  type Rect,
} from "./paneModel";
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
const DRAG_START_PX = 4;
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
export const PaneArea = memo(function PaneArea({
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
  const shown = wide && (size.w === 0 || fits(tree, size.w, size.h));

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
    const element = preview.current;
    if (!element) return;
    if (!target?.preview) {
      element.dataset.show = "false";
      return;
    }
    Object.assign(element.style, {
      left: `${target.preview.x}px`,
      top: `${target.preview.y}px`,
      width: `${target.preview.w}px`,
      height: `${target.preview.h}px`,
    });
    element.dataset.show = "true";
    element.dataset.refused = String(!target.ok);
  };

  /** Ends the drag in progress; `true` drops the pane where the pointer is. */
  const stopDrag = useRef<((commit: boolean) => void) | null>(null);
  useEffect(() => () => stopDrag.current?.(false), []);

  const startDrag = (pane: Pane) => (event: PointerEvent<HTMLElement>) => {
    const target = event.target as Element;
    if (event.button !== 0 || stopDrag.current) return;
    if (!target.closest(".pane-head, .panel-head")) return;
    if (target.closest("button, a, input, [role=slider]")) return;
    const slot = event.currentTarget;
    const pointer = event.pointerId;
    const box = slot.getBoundingClientRect();
    const size = {
      w: Math.min(box.width, GHOST_MAX.w),
      h: Math.min(box.height, GHOST_MAX.h),
    };
    const grab = {
      x: ((event.clientX - box.left) * size.w) / box.width,
      y: Math.min(event.clientY - box.top, size.h - 8),
    };
    const start = { x: event.clientX, y: event.clientY };
    const before = layout;
    let at = start;
    let bounds: DOMRect | null = null;
    let targets: Target[] = [];
    let hit: Target | null = null;
    let frame = 0;

    const hitNow = () =>
      bounds ? hitTarget(targets, at.x - bounds.left, at.y - bounds.top) : null;
    const paint = () => {
      frame = 0;
      if (ghost.current)
        ghost.current.style.transform = `translate3d(${at.x - grab.x}px, ${at.y - grab.y}px, 0)`;
      const next = hitNow();
      if (next?.id !== hit?.id) showPreview(next);
      hit = next;
    };
    const begin = () => {
      bounds = area.current?.getBoundingClientRect() ?? null;
      if (!bounds) return;
      targets = dockTargets(before, pane, {
        w: bounds.width,
        h: bounds.height,
      });
      slot.setPointerCapture(pointer);
      slot.dataset.source = "";
      document.body.classList.add("pane-dragging");
      const element = ghost.current;
      if (!element) return;
      element.querySelector(".pane-ghost-title")!.textContent =
        views[pane]?.title ?? "";
      element.style.width = `${size.w}px`;
      element.style.height = `${size.h}px`;
      element.dataset.show = "true";
    };
    const move = (next: globalThis.PointerEvent) => {
      if (next.pointerId !== pointer) return;
      at = { x: next.clientX, y: next.clientY };
      if (!bounds) {
        if (Math.hypot(at.x - start.x, at.y - start.y) < DRAG_START_PX) return;
        begin();
        if (!bounds) return;
      }
      if (!frame) frame = requestAnimationFrame(paint);
    };
    const up = (next: globalThis.PointerEvent) => {
      if (next.pointerId === pointer) stop(true);
    };
    const cancel = () => stop(false);
    const key = (next: globalThis.KeyboardEvent) => {
      if (next.key !== "Escape" || !bounds) return;
      next.preventDefault();
      next.stopPropagation();
      stop(false);
    };
    const stop = (commit: boolean) => {
      stopDrag.current = null;
      cancelAnimationFrame(frame);
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
      window.removeEventListener("pointercancel", cancel);
      window.removeEventListener("keydown", key, true);
      if (!bounds) return;
      delete slot.dataset.source;
      document.body.classList.remove("pane-dragging");
      if (ghost.current) ghost.current.dataset.show = "false";
      showPreview(null);
      if (!commit) return;
      const dropped = hitNow();
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
            x: at.x - grab.x - bounds.left,
            y: at.y - grab.y - bounds.top,
            ...size,
          },
          { w: bounds.width, h: bounds.height },
        ),
      );
    };
    stopDrag.current = stop;
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
    window.addEventListener("pointercancel", cancel);
    window.addEventListener("keydown", key, true);
  };

  const dividerDrag =
    (divider: Divider) => (event: PointerEvent<HTMLElement>) => {
      const element = area.current;
      if (event.button !== 0 || !element) return;
      const target = event.currentTarget;
      target.setPointerCapture(event.pointerId);
      const box = element.getBoundingClientRect();
      const base = layout.tree;
      let next: PaneNode | null = null;
      let frame = 0;
      let at = 0;
      const paintDivider = () => {
        frame = 0;
        const { x, y, w, h } = divider.box;
        const raw =
          divider.axis === "horizontal"
            ? ((at - box.left - x * box.width) / (w * box.width)) * 100
            : ((at - box.top - y * box.height) / (h * box.height)) * 100;
        const candidate = setSplitPercent(
          base,
          divider.index,
          snapPercent(raw),
        );
        if (candidate !== next && fits(candidate, box.width, box.height)) {
          next = candidate;
          setLive(candidate);
        }
      };
      const move = (move: globalThis.PointerEvent) => {
        at = divider.axis === "horizontal" ? move.clientX : move.clientY;
        if (!frame) frame = requestAnimationFrame(paintDivider);
      };
      const up = () => {
        if (frame) {
          cancelAnimationFrame(frame);
          paintDivider();
        }
        window.removeEventListener("pointermove", move);
        window.removeEventListener("pointerup", up);
        window.removeEventListener("pointercancel", up);
        document.body.classList.remove("pane-dragging");
        setLive(null);
        if (next && next !== base) onLayout({ ...layout, tree: next });
      };
      document.body.classList.add("pane-dragging");
      window.addEventListener("pointermove", move);
      window.addEventListener("pointerup", up);
      window.addEventListener("pointercancel", up);
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
                  <header className="pane-head" title={s.drag_pane}>
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
                aria-valuemin={10}
                aria-valuemax={90}
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
});
