import { type Audio, clamp, type Peak, type TrackView } from "./screenModel";
import { rgba, type Rgb } from "./visualizerModel";

/** What a style keeps from one frame to the next; a new style starts fresh. */
interface PaintState {
  peaks: Peak[];
  needles: [number, number];
  trails: number[][];
  phase: number;
  reels: [number, number];
  meter: number;
}

export function freshState(): PaintState {
  return {
    peaks: [],
    needles: [0, 0],
    trails: [],
    phase: 0,
    reels: [0, 30],
    meter: 0,
  };
}

/** One frame to paint, in CSS pixels. */
export interface Scene {
  ctx: CanvasRenderingContext2D;
  w: number;
  h: number;
  audio: Audio;
  track: TrackView;
  /** Milliseconds, and the seconds since the last frame. */
  now: number;
  dt: number;
  accent: Rgb;
  state: PaintState;
}

const MONO = "'JetBrains Mono', ui-monospace, monospace";

export function font(px: number, bold = false): string {
  return `${bold ? "700 " : ""}${Math.max(1, Math.round(px))}px ${MONO}`;
}

export function textWidth(
  ctx: CanvasRenderingContext2D,
  value: string,
  px: number,
  bold = false,
): number {
  ctx.font = font(px, bold);
  return ctx.measureText(value).width;
}

/** Draws `value` centred on `y`. */
export function text(
  ctx: CanvasRenderingContext2D,
  value: string,
  x: number,
  y: number,
  colour: Rgb,
  px: number,
  {
    align = "left",
    bold = false,
  }: { align?: CanvasTextAlign; bold?: boolean } = {},
): void {
  ctx.font = font(px, bold);
  ctx.textAlign = align;
  ctx.textBaseline = "middle";
  ctx.fillStyle = rgba(colour);
  ctx.fillText(value, x, y);
}

/** Starts a path through `count` points at `x(i)`, `y(i)`; the caller strokes or fills it. */
export function polyline(
  ctx: CanvasRenderingContext2D,
  count: number,
  x: (i: number) => number,
  y: (i: number) => number,
): void {
  ctx.beginPath();
  for (let i = 0; i < count; i++)
    if (i === 0) ctx.moveTo(x(i), y(i));
    else ctx.lineTo(x(i), y(i));
}

/** Strokes the current path with a glow: wide and faint, then thin. Cheaper than `shadowBlur` on long paths. */
export function glowStroke(
  ctx: CanvasRenderingContext2D,
  colour: Rgb,
  width: number,
  glow: number,
): void {
  ctx.strokeStyle = rgba(colour, 0.25);
  ctx.lineWidth = width + glow;
  ctx.stroke();
  ctx.strokeStyle = rgba(colour);
  ctx.lineWidth = width;
  ctx.stroke();
}

export function roundRect(
  ctx: CanvasRenderingContext2D,
  x: number,
  y: number,
  w: number,
  h: number,
  r: number,
): void {
  const k = Math.max(0, Math.min(r, w / 2, h / 2));
  ctx.beginPath();
  ctx.moveTo(x + k, y);
  ctx.arcTo(x + w, y, x + w, y + h, k);
  ctx.arcTo(x + w, y + h, x, y + h, k);
  ctx.arcTo(x, y + h, x, y, k);
  ctx.arcTo(x, y, x + w, y, k);
  ctx.closePath();
}

const FLAT = [0, 0];

/** The wave to trace: silence is a flat line. */
export function waveOf(scene: Scene): number[] {
  return scene.audio.wave.length > 1 ? scene.audio.wave : FLAT;
}

/** The faceplates' grid: one terminal row scaled to the canvas, a cell's width, and the text size. */
export function grid(h: number): { row: number; col: number; px: number } {
  const row = clamp(h / 30, 10, 22);
  return { row, col: row * 0.6, px: row * 0.8 };
}
