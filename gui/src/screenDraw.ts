import { type Audio, clamp, type Peak, type TrackView } from "./screenModel";
import { rgba, type Rgb } from "./visualizerModel";

/** What a style keeps from one frame to the next; a new style starts fresh. */
export interface PaintState {
  frames: number;
  peaks: Peak[];
  needles: [number, number];
  trails: number[][];
  phase: number;
  reels: [number, number];
  meter: number;
}

export function freshState(): PaintState {
  return {
    frames: 0,
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
  /** The bands the style asked for: what silence shows. */
  bars: number;
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

/** Starts a path through `points`; the caller strokes or fills it. */
export function polyline(
  ctx: CanvasRenderingContext2D,
  points: [number, number][],
): void {
  ctx.beginPath();
  points.forEach(([x, y], i) =>
    i === 0 ? ctx.moveTo(x, y) : ctx.lineTo(x, y),
  );
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

/** The bands to draw: silence rests at the count the style asked for. */
export function bandsOf(scene: Scene): number[] {
  return scene.audio.bands.length > 0
    ? scene.audio.bands
    : new Array<number>(Math.max(scene.bars, 2)).fill(0);
}

/** The faceplates' grid: one terminal row scaled to the canvas, a cell's width, and the text size. */
export function grid(h: number): { row: number; col: number; px: number } {
  const row = clamp(h / 30, 10, 22);
  return { row, col: row * 0.6, px: row * 0.8 };
}
