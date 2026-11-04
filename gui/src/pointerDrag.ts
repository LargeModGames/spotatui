import type { PointerEvent as ReactPointerEvent } from "react";
import type { Rect } from "./paneModel";

export interface Point {
  x: number;
  y: number;
}

/**
 * Follows the pointer of a press: a move past `threshold` pixels starts the drag, Escape or
 * a cancelled pointer ends it without a drop. `end` gets the drop point, or null.
 * Returns the function that cancels the drag.
 */
export function trackDrag(
  event: ReactPointerEvent<Element>,
  drag: {
    threshold?: number;
    /** The cursor over the whole page while the drag lasts. */
    cursor?: string;
    /** False keeps the press from starting the drag on this move. */
    begin?: () => boolean;
    /** Once per animation frame while the pointer moves. */
    paint: (at: Point) => void;
    end: (drop: Point | null) => void;
  },
): () => void {
  const { threshold = 4, cursor = "grabbing" } = drag;
  const handle = event.currentTarget;
  const pointer = event.pointerId;
  const start = { x: event.clientX, y: event.clientY };
  let at = start;
  let started = false;
  let frame = 0;
  const paint = () => {
    frame = 0;
    drag.paint(at);
  };
  const move = (next: PointerEvent) => {
    if (next.pointerId !== pointer) return;
    at = { x: next.clientX, y: next.clientY };
    if (!started) {
      if (Math.hypot(at.x - start.x, at.y - start.y) < threshold) return;
      if (drag.begin?.() === false) return;
      started = true;
      handle.setPointerCapture(pointer);
      document.body.classList.add("pane-dragging");
      document.body.style.setProperty("--drag-cursor", cursor);
    }
    if (!frame) frame = requestAnimationFrame(paint);
  };
  const up = (next: PointerEvent) => {
    if (next.pointerId === pointer) stop(true);
  };
  const cancel = () => stop(false);
  const key = (next: KeyboardEvent) => {
    if (next.key !== "Escape" || !started) return;
    next.preventDefault();
    next.stopPropagation();
    stop(false);
  };
  const stop = (commit: boolean) => {
    cancelAnimationFrame(frame);
    window.removeEventListener("pointermove", move);
    window.removeEventListener("pointerup", up);
    window.removeEventListener("pointercancel", cancel);
    window.removeEventListener("keydown", key, true);
    if (started) {
      document.body.classList.remove("pane-dragging");
      document.body.style.removeProperty("--drag-cursor");
    }
    drag.end(started && commit ? at : null);
  };
  window.addEventListener("pointermove", move);
  window.addEventListener("pointerup", up);
  window.addEventListener("pointercancel", cancel);
  window.addEventListener("keydown", key, true);
  return cancel;
}

/** Shows a preview element at `rect` in pixels, or hides it for null. */
export function placeBox(element: HTMLElement | null, rect: Rect | null) {
  if (!element) return;
  element.dataset.show = String(rect !== null);
  if (!rect) return;
  Object.assign(element.style, {
    left: `${rect.x}px`,
    top: `${rect.y}px`,
    width: `${rect.w}px`,
    height: `${rect.h}px`,
  });
}
