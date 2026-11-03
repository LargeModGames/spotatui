import { clock } from "./format";
import {
  font,
  grid,
  polyline,
  type Scene,
  text,
  textWidth,
} from "./screenDraw";
import {
  clamp,
  FLOOR,
  HEAD_UNIT_BANDS,
  HEAD_UNIT_MARKS,
  ledColours,
  markBand,
  mono,
  type Point,
  Projection,
  stepMeter,
  stripColour,
  type Style,
  type TrackView,
  VFD,
} from "./screenModel";
import { mix, rgba, type Rgb } from "./visualizerModel";
import {
  paintAnalogVu,
  paintCassette,
  paintOutrun,
  paintPhosphorScope,
  paintRackEq,
  paintWinamp,
} from "./screenRetro";

export const PAINTERS: Record<Style, (scene: Scene) => void> = {
  Cava: paintCava,
  BarGraph: paintBarGraph,
  HeadUnit: paintHeadUnit,
  AnalogVu: paintAnalogVu,
  Winamp: paintWinamp,
  PhosphorScope: paintPhosphorScope,
  Outrun: paintOutrun,
  RackEq: paintRackEq,
  Cassette: paintCassette,
};

// Cava: cava's mirrored bars, about two parts bar to one part gap, in the accent.

function paintCava(scene: Scene): void {
  const { ctx, w, h, accent } = scene;
  const bands = scene.audio.bands;
  const footprint = w / bands.length;
  const gap = Math.max(footprint / 3, 1);
  const bar = Math.max(footprint - gap, 1);
  const rest = Math.max(h / 120, 1.5);
  ctx.fillStyle = rgba(accent);
  bands.forEach((value, i) => {
    const height = Math.max(clamp(value, 0, 1) * h, rest);
    ctx.fillRect(i * footprint + gap / 2, h - height, bar, height);
  });
}

// BarGraph: thin bars side by side under a vertical turbo gradient.

/** Google's polynomial fit of the turbo colour map. */
export function turbo(t: number): Rgb {
  const x = clamp(t, 0, 1);
  const poly = (c: number[]) =>
    Math.round(
      clamp(
        c[0] + x * (c[1] + x * (c[2] + x * (c[3] + x * (c[4] + x * c[5])))),
        0,
        1,
      ) * 255,
    );
  return [
    poly([0.1357, 4.6154, -42.6603, 132.1311, -152.9424, 59.2864]),
    poly([0.0914, 2.1942, 4.843, -14.185, 4.2773, 2.8296]),
    poly([0.1067, 12.6419, -60.582, 110.3628, -89.9031, 27.3482]),
  ];
}

function paintBarGraph(scene: Scene): void {
  const { ctx, w, h } = scene;
  const bands = scene.audio.bands;
  const gradient = ctx.createLinearGradient(0, h, 0, 0);
  for (let i = 0; i <= 8; i++) gradient.addColorStop(i / 8, rgba(turbo(i / 8)));
  ctx.fillStyle = gradient;
  const step = w / bands.length;
  bands.forEach((value, i) => {
    const height = clamp(value, 0, 1) * h;
    if (height > 0)
      ctx.fillRect(i * step, h - height, Math.max(step - 0.5, 0.5), height);
  });
}

// Head Unit: a 2000s car-stereo VFD spectrum in perspective.

const CHROME = {
  low: [0, 200, 200] as Rgb,
  mid: [180, 0, 180] as Rgb,
  high: [200, 200, 0] as Rgb,
  hot: [255, 100, 100] as Rgb,
  cap: [220, 220, 220] as Rgb,
  bg: [16, 16, 16] as Rgb,
  label: [128, 128, 128] as Rgb,
};
/** An unlit VFD segment: a tenth of the way from the background to its colour. */
const GHOST = 0.1;
const ghost = (colour: Rgb) => mix(CHROME.bg, colour, GHOST);
const DIGITS = ledColours(CHROME.bg, [CHROME.cap, ghost(CHROME.cap)], GHOST);
const CHIP_OFF = ghost(CHROME.high);

/** The lane of band `b` across the floor. */
function lane(b: number): [number, number] {
  const pad = 0.225;
  return [(b + pad) / HEAD_UNIT_BANDS, (b + 1 - pad) / HEAD_UNIT_BANDS];
}

function paintHeadUnit(scene: Scene): void {
  const { ctx, w, h, state } = scene;
  if (w < 120 || h < 60) return;
  const live = mono(scene.audio.bands);
  state.meter = stepMeter(state.meter, live, scene.dt);
  const { row, col, px } = grid(h);
  ctx.fillStyle = rgba(CHROME.bg);
  ctx.fillRect(0, 0, w, h);

  const header = h >= 12 * row;
  const readout = h >= 15 * row && w >= 44 * col;
  const labels = h >= 8 * row;
  const meter = w >= 40 * col;
  let top = 0;
  let bottom = h;
  if (header) {
    drawHeader(ctx, scene.track, w, row, col, px);
    top = row * 2;
  }
  if (readout) {
    drawReadout(ctx, scene.track, state.meter, w, h - row * 3, row, col, px);
    bottom = h - row * 4;
  }
  if (labels) bottom -= row;
  let left = 0;
  if (meter) {
    drawMeter(ctx, state.meter, 0, top, col * 2, bottom - top, row);
    left = col * 3;
  }
  if (bottom - top < row * 2) return;
  const stage = { x: left, y: top, w: w - left, h: bottom - top };
  const floor = new Projection(
    FLOOR.map(([fx, fy]) => [
      stage.x + fx * stage.w,
      stage.y + fy * stage.h,
    ]) as [Point, Point, Point, Point],
  );
  ctx.fillStyle = rgba(VFD.glass);
  ctx.fillRect(stage.x, stage.y, stage.w, stage.h);
  drawWalls(ctx, stage, floor);
  live.forEach((level, b) => drawStrip(ctx, floor, b, clamp(level, 0, 1), row));
  // The display's dot rows: thin dark lines over the whole glass.
  ctx.fillStyle = rgba(VFD.glass, 0.55);
  ctx.beginPath();
  for (let y = stage.y; y < stage.y + stage.h; y += 4)
    ctx.rect(stage.x, y, stage.w, 1);
  ctx.fill();
  if (labels) drawMarks(ctx, floor, bottom + row / 2, stage, px);
}

function drawStrip(
  ctx: CanvasRenderingContext2D,
  floor: Projection,
  b: number,
  level: number,
  row: number,
): void {
  const [u0, u1] = lane(b);
  const uc = (u0 + u1) / 2;
  const front = floor.apply(uc, 0);
  const back = floor.apply(uc, 1);
  const width = Math.max(floor.apply(u1, 0.5)[0] - floor.apply(u0, 0.5)[0], 2);
  const depth = front[1] - back[1];
  if (depth <= 0) return;
  const segment = Math.max(row / 3.5, 3);
  const gap = Math.max(segment * 0.22, 1);
  for (let y = front[1]; y > back[1]; y -= segment) {
    const yTop = Math.max(y - segment + gap, back[1]);
    const mid = (y + yTop) / 2;
    ctx.fillStyle = stripColour((front[1] - mid) / depth, level);
    const at = (yy: number) =>
      front[0] + ((front[1] - yy) / depth) * (back[0] - front[0]);
    ctx.beginPath();
    ctx.moveTo(at(y) - width / 2, y);
    ctx.lineTo(at(y) + width / 2, y);
    ctx.lineTo(at(yTop) + width / 2, yTop);
    ctx.lineTo(at(yTop) - width / 2, yTop);
    ctx.closePath();
    ctx.fill();
  }
}

function drawWalls(
  ctx: CanvasRenderingContext2D,
  stage: { x: number; y: number; w: number; h: number },
  floor: Projection,
): void {
  const { x, y, w, h } = stage;
  ctx.save();
  // Walls stand behind the floor, never on it.
  const corners = [
    floor.apply(0, 0),
    floor.apply(1, 0),
    floor.apply(1, 1),
    floor.apply(0, 1),
  ];
  polyline(
    ctx,
    4,
    (i) => corners[i][0],
    (i) => corners[i][1],
  );
  ctx.closePath();
  ctx.rect(x, y, w, h);
  ctx.clip("evenodd");
  const wall = (
    x0: number,
    x1: number,
    top: number,
    bottomLeft: number,
    bottomRight: number,
  ) => {
    const gradient = ctx.createLinearGradient(
      0,
      top,
      0,
      Math.max(bottomLeft, bottomRight),
    );
    gradient.addColorStop(0, rgba(VFD.wallTop));
    gradient.addColorStop(1, rgba(VFD.wallBottom));
    ctx.fillStyle = gradient;
    const xs = [x0, x1, x1, x0];
    const ys = [top, top, bottomRight, bottomLeft];
    polyline(
      ctx,
      4,
      (i) => xs[i],
      (i) => ys[i],
    );
    ctx.closePath();
    ctx.fill();
  };
  for (let k = 0; k < 6; k++) {
    const x0 = floor.apply(lane(2 * k)[0], 1)[0];
    const x1 = floor.apply(lane(2 * k + 1)[1], 1)[0];
    const top = y + (0.02 + ((0.11 - 0.02) * k) / 5) * h;
    wall(x0, x1, top, y + 0.31 * h, y + 0.31 * h);
  }
  for (let panel = 0; panel < 3; panel++) {
    const t0 = (panel + 0.11) / 3;
    const t1 = (panel + 0.89) / 3;
    const at = (t: number) => x + (0.835 + 0.16 * t) * w;
    const base = (t: number) => y + (0.34 + 0.4 * t) * h;
    wall(at(t0), at(t1), y + 0.03 * h, base(t0), base(t1));
  }
  ctx.restore();
}

function drawMarks(
  ctx: CanvasRenderingContext2D,
  floor: Projection,
  y: number,
  stage: { x: number; w: number },
  px: number,
): void {
  let edge = -Infinity;
  for (const [hz, label] of HEAD_UNIT_MARKS) {
    const x = floor.apply((markBand(hz) + 0.5) / HEAD_UNIT_BANDS, 0)[0];
    const half = textWidth(ctx, label, px) / 2;
    if (x - half < edge + px * 0.6 || x + half > stage.x + stage.w) continue;
    text(ctx, label, x, y, CHROME.label, px, { align: "center" });
    edge = x + half;
  }
}

const METER_COLOURS = new Map<number, { lit: string; dim: string }[]>();

/** Each segment's lit and unlit colour for a meter of `count` segments, built once per height. */
function meterColours(count: number): { lit: string; dim: string }[] {
  let colours = METER_COLOURS.get(count);
  if (!colours) {
    const bases = Array.from({ length: count }, (_, segment) => {
      const t = segment / (count - 1);
      return t >= 0.8 ? CHROME.hot : mix(CHROME.low, CHROME.mid, t);
    });
    colours = ledColours(CHROME.bg, bases, GHOST);
    METER_COLOURS.set(count, colours);
  }
  return colours;
}

function drawMeter(
  ctx: CanvasRenderingContext2D,
  level: number,
  x: number,
  y: number,
  w: number,
  h: number,
  row: number,
): void {
  const pitch = row / 2;
  const count = Math.max(Math.floor(h / pitch), 2);
  const lit = Math.round(level * count);
  const colours = meterColours(count);
  for (let segment = 0; segment < count; segment++) {
    ctx.fillStyle = segment < lit ? colours[segment].lit : colours[segment].dim;
    const top = y + h - (segment + 1) * pitch;
    ctx.fillRect(x, top + pitch * 0.25, w, pitch * 0.75);
  }
}

function drawHeader(
  ctx: CanvasRenderingContext2D,
  track: TrackView,
  w: number,
  row: number,
  col: number,
  px: number,
): void {
  const y = row / 2;
  text(ctx, "DSP HEAD UNIT", col, y, CHROME.label, px);
  const left = textWidth(ctx, "DSP HEAD UNIT", px);
  const chipWidth = col * 5;
  const chipX = w - col - chipWidth;
  chip(ctx, "DHE", chipX, y, chipWidth, row, true);
  if (!track.title) return;
  const start = col + left + col * 2;
  const room = chipX - col - start;
  if (room <= 0) return;
  ctx.font = font(px, true);
  const spaced = [...track.title.toUpperCase()].join(" ");
  let title =
    ctx.measureText(spaced).width <= room ? spaced : track.title.toUpperCase();
  while (title.length > 1 && ctx.measureText(title).width > room)
    title = title.slice(0, -1);
  const width = ctx.measureText(title).width;
  const x = Math.max(start, (w - width) / 2);
  text(ctx, title, x, y, CHROME.mid, px, { bold: true });
}

function chip(
  ctx: CanvasRenderingContext2D,
  label: string,
  x: number,
  y: number,
  width: number,
  row: number,
  lit: boolean,
): void {
  if (lit) {
    ctx.fillStyle = rgba(CHROME.high);
    ctx.fillRect(x, y - row * 0.42, width, row * 0.84);
  }
  text(ctx, label, x + width / 2, y, lit ? CHROME.bg : CHIP_OFF, row * 0.72, {
    align: "center",
    bold: lit,
  });
}

/** The seven segments a-g of each glyph. */
const SEGMENTS: Record<string, string> = {
  "0": "abcdef",
  "1": "bc",
  "2": "abged",
  "3": "abgcd",
  "4": "fgbc",
  "5": "afgcd",
  "6": "afgedc",
  "7": "abc",
  "8": "abcdefg",
  "9": "abcdfg",
  "-": "g",
};

function sevenSegment(
  ctx: CanvasRenderingContext2D,
  glyph: string,
  x: number,
  y: number,
  w: number,
  h: number,
  colours: { lit: string; dim: string },
): void {
  const t = Math.max(w * 0.16, 1.5);
  const m = y + h / 2;
  const side = h / 2 - 1.5 * t;
  const on = SEGMENTS[glyph] ?? "";
  const bar = (
    name: string,
    bx: number,
    by: number,
    bw: number,
    bh: number,
  ) => {
    ctx.fillStyle = on.includes(name) ? colours.lit : colours.dim;
    ctx.fillRect(bx, by, bw, bh);
  };
  bar("a", x + t, y, w - 2 * t, t);
  bar("b", x + w - t, y + t, t, side);
  bar("c", x + w - t, m + t / 2, t, side);
  bar("d", x + t, y + h - t, w - 2 * t, t);
  bar("e", x, m + t / 2, t, side);
  bar("f", x, y + t, t, side);
  bar("g", x + t, m - t / 2, w - 2 * t, t);
}

function drawReadout(
  ctx: CanvasRenderingContext2D,
  track: TrackView,
  level: number,
  w: number,
  y: number,
  row: number,
  col: number,
  px: number,
): void {
  const idle = track.progressMs === null;
  const value = idle ? "--:--" : clock(track.progressMs ?? 0).padStart(5, "0");
  const colours = DIGITS[idle ? 1 : 0];
  let x = col * 3;
  for (const glyph of value) {
    if (glyph === ":") {
      ctx.fillStyle = colours.lit;
      ctx.fillRect(x + col * 0.25, y + row * 0.7, col * 0.5, col * 0.5);
      ctx.fillRect(x + col * 0.25, y + row * 2.0, col * 0.5, col * 0.5);
      x += col * 2;
      continue;
    }
    sevenSegment(ctx, glyph, x, y + row * 0.2, col * 2.4, row * 2.6, colours);
    x += col * 3.4;
  }
  x += col;
  text(ctx, "ST", x, y + row / 2, CHROME.mid, px);
  const state = track.live
    ? "LIVE"
    : track.playing
      ? "PLAY"
      : idle
        ? "STOP"
        : "PAUSE";
  text(ctx, state, x, y + row * 2.5, CHROME.label, px);
  const chips: [string, boolean][] = [
    ["CLIP", level > 0.92],
    ["RPT", track.repeat],
    ["SHUF", track.shuffle],
  ];
  let right = w - col;
  for (const [label, on] of chips) {
    const width = col * (label.length + 2);
    if (right - width < x + col * 6) break;
    chip(ctx, label, right - width, y + row * 1.5, width, row, on);
    right -= width + col;
  }
}
