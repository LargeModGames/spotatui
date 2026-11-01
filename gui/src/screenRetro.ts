import { clock } from "./format";
import {
  bandsOf,
  glowStroke,
  grid,
  polyline,
  roundRect,
  type Scene,
  text,
  textWidth,
  waveOf,
} from "./screenDraw";
import {
  channels,
  clamp,
  levels,
  mono,
  pushSweep,
  RACK_BANDS,
  RACK_LABELS,
  resample,
  scopeReadout,
  stepNeedle,
  stepOutrun,
  stepReels,
  tapePacks,
  tapeProgress,
  TRAILS,
  updatePeaks,
  VU_MARKS,
  VU_ZERO,
  vuDb,
} from "./screenModel";
import { mix, rgba, type Rgb } from "./visualizerModel";

const rgb = (r: number, g: number, b: number): Rgb => [r, g, b];
const DEG = Math.PI / 180;

/** Each LED colour lit, and unlit over `base`, built once. */
function ledColours(base: Rgb, colours: Rgb[]): { lit: string; dim: string }[] {
  return colours.map((colour) => ({
    lit: rgba(colour),
    dim: rgba(mix(base, colour, 0.18)),
  }));
}

// Analog VU: two backlit 1970s hi-fi needle meters.

const VU = {
  case: rgb(14, 10, 6),
  face: rgb(232, 200, 130),
  faceEdge: rgb(190, 150, 80),
  ink: rgb(40, 28, 18),
  needle: rgb(20, 14, 10),
  red: rgb(186, 40, 30),
  label: rgb(150, 120, 80),
  title: rgb(210, 170, 100),
  power: rgb(240, 170, 60),
  footer: rgb(120, 95, 60),
  unit: rgb(110, 80, 50),
};

const signed = (db: number) => {
  const whole = Math.round(db);
  return (whole >= 0 ? `+${whole}` : `${whole}`).padStart(3);
};

export function paintAnalogVu(scene: Scene): void {
  const { ctx, w, h, state, dt } = scene;
  if (w < 160 || h < 100) return;
  const [left, right] = levels(bandsOf(scene));
  state.needles = [
    stepNeedle(state.needles[0], left, dt),
    stepNeedle(state.needles[1], right, dt),
  ];
  const { row, col, px } = grid(h);
  ctx.fillStyle = rgba(VU.case);
  ctx.fillRect(0, 0, w, h);
  text(ctx, "STEREO VU", col * 2, row * 0.7, VU.title, px, { bold: true });
  const peak = state.needles.some((needle) => needle > VU_ZERO);
  lamp(ctx, "POWER", w - col * 3, row * 0.7, VU.power, true, px);
  lamp(ctx, "PEAK", w - col * 13, row * 0.7, VU.red, peak, px);
  const margin = col * 3;
  const gap = col * 4;
  const faceW = (w - margin * 2 - gap) / 2;
  // A real meter face is wider than tall, so the needle's pivot sits under it.
  const faceH = Math.min(h - row * 4, faceW * 0.72);
  if (faceW < 60 || faceH < 40) return;
  const faceY = row * 1.6 + (h - row * 4 - faceH) / 2;
  state.needles.forEach((needle, i) =>
    drawVuFace(
      ctx,
      margin + i * (faceW + gap),
      faceY,
      faceW,
      faceH,
      needle,
      i === 0 ? "L" : "R",
      row,
    ),
  );
  const [l, r] = state.needles.map(vuDb);
  text(
    ctx,
    `L ${signed(l)} VU    R ${signed(r)} VU`,
    col * 2,
    h - row * 1.1,
    VU.footer,
    px,
  );
}

function lamp(
  ctx: CanvasRenderingContext2D,
  label: string,
  right: number,
  y: number,
  colour: Rgb,
  lit: boolean,
  px: number,
): void {
  const radius = px * 0.38;
  ctx.fillStyle = rgba(lit ? colour : mix(VU.case, colour, 0.3));
  if (lit) {
    ctx.shadowColor = rgba(colour);
    ctx.shadowBlur = px;
  }
  ctx.beginPath();
  ctx.arc(right - radius, y, radius, 0, Math.PI * 2);
  ctx.fill();
  ctx.shadowBlur = 0;
  text(ctx, label, right - radius * 3, y, VU.label, px, { align: "right" });
}

function drawVuFace(
  ctx: CanvasRenderingContext2D,
  x: number,
  y: number,
  w: number,
  h: number,
  needle: number,
  side: string,
  row: number,
): void {
  const glow = ctx.createRadialGradient(
    x + w / 2,
    y + h * 0.55,
    0,
    x + w / 2,
    y + h * 0.55,
    w * 0.7,
  );
  glow.addColorStop(0, rgba(mix(VU.face, rgb(255, 240, 200), 0.35)));
  glow.addColorStop(1, rgba(VU.faceEdge));
  roundRect(ctx, x, y, w, h, row * 0.5);
  ctx.fillStyle = glow;
  ctx.fill();
  ctx.lineWidth = Math.max(row * 0.15, 1.5);
  ctx.strokeStyle = rgba(VU.faceEdge);
  ctx.stroke();
  ctx.save();
  roundRect(ctx, x, y, w, h, row * 0.5);
  ctx.clip();
  const radius = (0.4 * w) / Math.cos(55 * DEG);
  const cx = x + w / 2;
  const cy = y + h * 0.42 + radius;
  const angle = (position: number) => (125 - 70 * clamp(position, 0, 1)) * DEG;
  const at = (position: number, r: number): [number, number] => [
    cx + r * Math.cos(angle(position)),
    cy - r * Math.sin(angle(position)),
  ];
  ctx.lineWidth = Math.max(row * 0.08, 1);
  ctx.strokeStyle = rgba(VU.ink);
  ctx.beginPath();
  ctx.arc(cx, cy, radius, -angle(0), -angle(1));
  ctx.stroke();
  ctx.lineWidth = row * 0.3;
  ctx.strokeStyle = rgba(VU.red);
  ctx.beginPath();
  ctx.arc(cx, cy, radius + row * 0.4, -angle(VU_ZERO), -angle(1));
  ctx.stroke();
  for (const [db, position] of VU_MARKS) {
    const colour = position >= VU_ZERO ? VU.red : VU.ink;
    const [x0, y0] = at(position, radius + row * 0.1);
    const [x1, y1] = at(position, radius + row * 0.7);
    ctx.lineWidth = Math.max(row * 0.08, 1);
    ctx.strokeStyle = rgba(colour);
    ctx.beginPath();
    ctx.moveTo(x0, y0);
    ctx.lineTo(x1, y1);
    ctx.stroke();
    const [lx, ly] = at(position, radius + row * 1.35);
    text(ctx, db > 0 ? `+${db}` : `${db}`, lx, ly, colour, row * 0.7, {
      align: "center",
      bold: true,
    });
  }
  text(ctx, "VU", cx, y + h * 0.7, VU.ink, row * 1.1, {
    align: "center",
    bold: true,
  });
  text(ctx, side, x + row * 0.7, y + h - row * 0.8, VU.label, row * 0.8, {
    bold: true,
  });
  text(
    ctx,
    "dB · 1kHz",
    x + w - row * 0.7,
    y + h - row * 0.8,
    VU.unit,
    row * 0.7,
    { align: "right" },
  );
  const [nx, ny] = at(needle, radius + row * 0.8);
  ctx.shadowColor = "rgba(0,0,0,0.35)";
  ctx.shadowBlur = row * 0.3;
  ctx.shadowOffsetX = row * 0.15;
  ctx.lineWidth = Math.max(row * 0.1, 1.5);
  ctx.strokeStyle = rgba(VU.needle);
  ctx.beginPath();
  ctx.moveTo(cx, cy);
  ctx.lineTo(nx, ny);
  ctx.stroke();
  ctx.restore();
  ctx.shadowBlur = 0;
  ctx.shadowOffsetX = 0;
}

// Winamp: the classic player's scanlined bars, peak caps and scope strip.

const AMP = {
  black: rgb(0, 0, 0),
  titleBar: rgb(32, 34, 52),
  lcd: rgb(0, 230, 0),
  green: rgb(0, 200, 0),
  yellow: rgb(210, 210, 0),
  red: rgb(230, 40, 20),
  cap: rgb(170, 170, 190),
  dim: rgb(120, 120, 140),
  logo: rgb(220, 200, 120),
  buttons: rgb(150, 150, 170),
  osc: rgb(90, 90, 110),
};
const WINAMP_BARS = 19;

export function paintWinamp(scene: Scene): void {
  const { ctx, w, h, state, track } = scene;
  if (w < 160 || h < 100) return;
  const { row, col, px } = grid(h);
  ctx.fillStyle = rgba(AMP.black);
  ctx.fillRect(0, 0, w, h);
  ctx.fillStyle = rgba(AMP.titleBar);
  ctx.fillRect(0, 0, w, row * 1.1);
  text(ctx, "WINAMP", col, row * 0.55, AMP.logo, px, { bold: true });
  const logo = textWidth(ctx, "WINAMP", px, true);
  const length = track.durationMs ? ` (${clock(track.durationMs)})` : "";
  text(
    ctx,
    `·  1. ${track.title ?? "spotatui"}${length}`,
    col * 2 + logo,
    row * 0.55,
    AMP.lcd,
    px,
  );
  for (let i = 0; i < 3; i++) {
    ctx.fillStyle = rgba(AMP.buttons);
    ctx.fillRect(w - col * (2 + i * 2), row * 0.3, col * 1.2, row * 0.5);
  }
  text(
    ctx,
    clock(track.progressMs ?? 0).padStart(5),
    col * 2,
    row * 2.3,
    AMP.lcd,
    px * 2,
    { bold: true },
  );

  const values = resample(mono(bandsOf(scene)), WINAMP_BARS);
  state.peaks = updatePeaks(state.peaks, values, scene.now, scene.dt, 300, 0.7);
  const x0 = col * 2;
  const top = row * 3.8;
  const area = Math.max(h - top - row * 5.6, row * 2);
  const bottom = top + area;
  const pitch = (w - col * 4) / WINAMP_BARS;
  const barW = Math.max(pitch * 0.73, 1);
  const fill = ctx.createLinearGradient(0, bottom, 0, top);
  fill.addColorStop(0, rgba(AMP.green));
  fill.addColorStop(0.45, rgba(AMP.green));
  fill.addColorStop(0.45, rgba(AMP.yellow));
  fill.addColorStop(0.75, rgba(AMP.yellow));
  fill.addColorStop(0.75, rgba(AMP.red));
  fill.addColorStop(1, rgba(AMP.red));
  ctx.fillStyle = fill;
  const room = area - 4;
  values.forEach((value, i) => {
    const height = clamp(value, 0, 1) * room;
    ctx.fillRect(x0 + i * pitch, bottom - height, barW, height);
  });
  ctx.fillStyle = rgba(AMP.black);
  for (let y = top; y < bottom; y += 2) ctx.fillRect(x0, y, w - col * 4, 1);
  ctx.fillStyle = rgba(AMP.cap);
  state.peaks.forEach((peak, i) => {
    const y = bottom - clamp(peak.value, 0, 1) * room - 3;
    ctx.fillRect(x0 + i * pitch, y, barW, 1.5);
  });

  const oscY = h - row * 5;
  const oscH = row * 3.4;
  text(ctx, "OSC", col * 2, oscY + oscH / 2, AMP.osc, px * 0.85);
  const ox = col * 6;
  const ow = w - col * 8;
  // At most one point for each two pixels.
  const points = Math.max(Math.floor(ow / 2), 2);
  const raw = waveOf(scene);
  const wave = raw.length > points ? resample(raw, points) : raw;
  ctx.strokeStyle = rgba(AMP.lcd);
  ctx.lineWidth = 1.2;
  polyline(
    ctx,
    wave.length,
    (i) => ox + (i / (wave.length - 1)) * ow,
    (i) => oscY + oscH / 2 - clamp(wave[i], -1, 1) * (oscH / 2) * 0.9,
  );
  ctx.stroke();

  const y = h - row * 0.8;
  const paused = !track.playing && track.progressMs !== null;
  const buttons: [string, boolean][] = [
    ["⏮", false],
    ["▶", track.playing],
    ["‖", paused],
    ["■", false],
    ["⏭", false],
  ];
  buttons.forEach(([glyph, lit], i) =>
    text(ctx, glyph, col * (2 + i * 4), y, lit ? AMP.lcd : AMP.dim, px * 1.1, {
      bold: lit,
    }),
  );
  text(ctx, "shuffle", col * 24, y, track.shuffle ? AMP.lcd : AMP.dim, px);
  text(ctx, "repeat", col * 33, y, track.repeat ? AMP.lcd : AMP.dim, px);
}

// Phosphor Scope: a green CRT oscilloscope with phosphor trails.

const SCOPE = {
  bg: rgb(6, 14, 8),
  glow: rgb(14, 34, 18),
  grid: rgb(20, 60, 30),
  text: rgb(90, 200, 110),
  trace: rgb(120, 255, 140),
  label: rgb(60, 140, 80),
  bezel: rgb(60, 110, 70),
};

export function paintPhosphorScope(scene: Scene): void {
  const { ctx, w, h, state } = scene;
  if (w < 160 || h < 100) return;
  const { row, col, px } = grid(h);
  const glow = ctx.createRadialGradient(
    w / 2,
    h / 2,
    0,
    w / 2,
    h / 2,
    Math.max(w, h) * 0.6,
  );
  glow.addColorStop(0, rgba(SCOPE.glow));
  glow.addColorStop(1, rgba(SCOPE.bg));
  ctx.fillStyle = glow;
  ctx.fillRect(0, 0, w, h);
  text(
    ctx,
    "CH1  2V/div   TIME 5ms/div   TRIG ⟋ AUTO",
    col * 2,
    row * 0.7,
    SCOPE.text,
    px,
  );
  text(ctx, "PHOSPHOR  P31", w - col * 2, row * 0.7, SCOPE.label, px, {
    align: "right",
  });
  const sx = col * 4;
  const sy = row * 2;
  const sw = w - col * 8;
  const sh = h - row * 4.5;
  if (sw < 40 || sh < 30) return;
  ctx.strokeStyle = rgba(SCOPE.grid);
  ctx.lineWidth = 1;
  ctx.setLineDash([1, 3]);
  ctx.beginPath();
  for (let i = 0; i <= 10; i++) {
    ctx.moveTo(sx + (i / 10) * sw, sy);
    ctx.lineTo(sx + (i / 10) * sw, sy + sh);
  }
  for (let j = 0; j <= 8; j++) {
    ctx.moveTo(sx, sy + (j / 8) * sh);
    ctx.lineTo(sx + sw, sy + (j / 8) * sh);
  }
  ctx.stroke();
  ctx.setLineDash([]);
  ctx.beginPath();
  for (let i = 0; i <= 50; i++) {
    const x = sx + (i / 50) * sw;
    ctx.moveTo(x, sy + sh / 2 - 3);
    ctx.lineTo(x, sy + sh / 2 + 3);
  }
  ctx.stroke();
  roundRect(ctx, sx - 4, sy - 4, sw + 8, sh + 8, row * 0.8);
  ctx.strokeStyle = rgba(SCOPE.bezel);
  ctx.lineWidth = 2;
  ctx.stroke();

  state.trails = pushSweep(state.trails, waveOf(scene));
  const offset = TRAILS.length - state.trails.length;
  state.trails.forEach((sweep, k) => {
    polyline(
      ctx,
      sweep.length,
      (i) => sx + (i / Math.max(sweep.length - 1, 1)) * sw,
      (i) => sy + sh / 2 - clamp(sweep[i], -1, 1) * sh * 0.42,
    );
    if (k === state.trails.length - 1) {
      glowStroke(ctx, SCOPE.trace, 1.8, row * 0.5);
      return;
    }
    ctx.strokeStyle = rgba(mix(SCOPE.bg, SCOPE.trace, TRAILS[offset + k]));
    ctx.lineWidth = 1.3;
    ctx.stroke();
  });
  const { vpp, hz } = scopeReadout(scene.audio.wave);
  text(
    ctx,
    `Vpp ${vpp.toFixed(2)} V    f ${hz.toFixed(0)} Hz`,
    sx,
    h - row * 1.1,
    SCOPE.text,
    px,
  );
}

// Outrun: synthwave sun, spectrum skyline and a scrolling neon grid.

const OUT = {
  skyTop: rgb(22, 6, 40),
  skyBottom: rgb(110, 24, 96),
  ground: rgb(10, 4, 22),
  mountain: rgb(12, 4, 28),
  neon: rgb(0, 240, 255),
  neonDim: rgb(0, 150, 210),
  grid: rgb(255, 40, 200),
  gridFar: rgb(190, 30, 160),
  horizon: rgb(255, 90, 220),
  sunTop: rgb(255, 236, 110),
  sunBottom: rgb(255, 50, 150),
  title: rgb(255, 120, 220),
};
const RIDGE_POINTS = 48;

export function paintOutrun(scene: Scene): void {
  const { ctx, w, h, state, track } = scene;
  if (w < 160 || h < 100) return;
  const { row, col, px } = grid(h);
  const row1 = mono(bandsOf(scene));
  const bass = Math.max(0, ...row1.slice(0, 3));
  state.phase = stepOutrun(state.phase, scene.dt, track.playing);
  const hz = (h * 17) / 30;
  const sky = ctx.createLinearGradient(0, 0, 0, hz);
  sky.addColorStop(0, rgba(OUT.skyTop));
  sky.addColorStop(1, rgba(OUT.skyBottom));
  ctx.fillStyle = sky;
  ctx.fillRect(0, 0, w, hz);
  ctx.fillStyle = rgba(OUT.ground);
  ctx.fillRect(0, hz, w, h - hz);

  const cx = w / 2;
  const cy = hz - 2;
  const radius = hz * 0.68;
  ctx.save();
  ctx.beginPath();
  ctx.arc(cx, cy, radius, Math.PI, 0);
  ctx.closePath();
  ctx.clip();
  const sun = ctx.createLinearGradient(0, cy - radius, 0, cy);
  sun.addColorStop(0, rgba(OUT.sunTop));
  sun.addColorStop(1, rgba(OUT.sunBottom));
  ctx.fillStyle = sun;
  ctx.fillRect(cx - radius, cy - radius, radius * 2, radius);
  const period = radius / 9;
  for (let y = cy - radius * 0.55; y < cy; y += period) {
    const depth = (y - (cy - radius)) / radius;
    const gap = (period * (1 + ((depth - 0.45) / 0.55) * 4 + bass * 2)) / 7;
    ctx.fillStyle = rgba(mix(OUT.skyTop, OUT.skyBottom, y / hz));
    ctx.fillRect(cx - radius, y + period - gap, radius * 2, gap);
  }
  ctx.restore();

  const ridge = resample(row1, RIDGE_POINTS);
  const peaks = ridge.map((value, i): [number, number] => [
    (i / (RIDGE_POINTS - 1)) * w,
    hz - 1 - clamp(value, 0, 1) * hz * 0.44,
  ]);
  ctx.fillStyle = rgba(OUT.mountain);
  const ridgeLine = (drop: number) =>
    polyline(
      ctx,
      peaks.length,
      (i) => peaks[i][0],
      (i) => peaks[i][1] + drop,
    );
  ridgeLine(0);
  ctx.lineTo(w, hz);
  ctx.lineTo(0, hz);
  ctx.closePath();
  ctx.fill();
  ridgeLine(3);
  ctx.strokeStyle = rgba(OUT.neonDim);
  ctx.lineWidth = 1;
  ctx.stroke();
  ridgeLine(0);
  glowStroke(ctx, OUT.neon, 2, row * 0.5);

  ctx.save();
  ctx.beginPath();
  ctx.rect(0, hz, w, h - hz);
  ctx.clip();
  const far = (w * 9) / 232;
  const near = (w * 62) / 232;
  ctx.beginPath();
  for (let i = -9; i <= 9; i++) {
    ctx.moveTo(cx + i * far, hz);
    ctx.lineTo(cx + i * near, h);
  }
  glowStroke(ctx, OUT.grid, 1.2, row * 0.35);
  // One stroke for each colour, not one for each line.
  const rows: [number[], number[]] = [[], []];
  for (let k = 0; k < 8; k++) {
    const z = 1.25 * Math.pow(1.65, k - state.phase);
    if (z < 1) continue;
    const y = hz + (h - hz) / z;
    if (y - hz < 6) break;
    rows[z < 3 ? 0 : 1].push(y);
  }
  rows.forEach((ys, far) => {
    ctx.beginPath();
    for (const y of ys) {
      ctx.moveTo(0, y);
      ctx.lineTo(w, y);
    }
    glowStroke(ctx, far ? OUT.gridFar : OUT.grid, 1.2, row * 0.35);
  });
  ctx.restore();
  ctx.fillStyle = rgba(OUT.horizon);
  ctx.fillRect(0, hz - 0.75, w, 1.5);
  ctx.shadowColor = rgba(OUT.neon);
  ctx.shadowBlur = row * 0.5;
  text(ctx, "O U T R U N", col * 2, row, OUT.neon, px * 1.2, { bold: true });
  ctx.shadowBlur = 0;
  if (track.title)
    text(
      ctx,
      `♪ ${track.title.toUpperCase()}`,
      w - col * 2,
      row,
      OUT.title,
      px,
      { align: "right" },
    );
}

// Rack EQ: an 80s rack-unit analyzer with left and right LED ladders.

const RACK = {
  plate: rgb(168, 170, 172),
  plateEdge: rgb(140, 142, 145),
  screw: rgb(90, 90, 92),
  window: rgb(24, 24, 26),
  green: rgb(40, 230, 70),
  amber: rgb(250, 190, 30),
  red: rgb(250, 50, 40),
  label: rgb(150, 150, 150),
  heading: rgb(30, 30, 34),
  model: rgb(60, 60, 64),
  peak: rgb(255, 255, 255),
  knob: rgb(40, 40, 44),
  knobName: rgb(50, 50, 54),
};
const RACK_LEDS = ledColours(RACK.window, [RACK.green, RACK.amber, RACK.red]);

export function paintRackEq(scene: Scene): void {
  const { ctx, w, h, state } = scene;
  if (w < 160 || h < 100) return;
  const { row, col, px } = grid(h);
  const plate = ctx.createLinearGradient(0, 0, 0, h);
  plate.addColorStop(0, rgba(mix(RACK.plate, rgb(255, 255, 255), 0.15)));
  plate.addColorStop(1, rgba(RACK.plate));
  ctx.fillStyle = plate;
  ctx.fillRect(0, 0, w, h);
  ctx.fillStyle = rgba(RACK.plateEdge);
  ctx.fillRect(0, 0, w, row * 0.5);
  ctx.fillRect(0, h - row * 0.5, w, row * 0.5);
  for (const [sx, sy] of [
    [col * 1.5, row * 1.2],
    [w - col * 1.5, row * 1.2],
    [col * 1.5, h - row * 1.2],
    [w - col * 1.5, h - row * 1.2],
  ]) {
    ctx.fillStyle = rgba(RACK.screw);
    ctx.beginPath();
    ctx.arc(sx, sy, row * 0.35, 0, Math.PI * 2);
    ctx.fill();
    ctx.fillStyle = rgba(RACK.plateEdge);
    ctx.fillRect(sx - row * 0.25, sy - 0.75, row * 0.5, 1.5);
  }
  text(
    ctx,
    "GRAPHIC EQUALIZER  ·  SPECTRUM ANALYZER",
    col * 5,
    row * 1.6,
    RACK.heading,
    px,
    { bold: true },
  );
  text(ctx, "MODEL SA-1200", w - col * 5, row * 1.6, RACK.model, px, {
    align: "right",
  });

  const wx = col * 4;
  const wy = row * 2.8;
  const ww = w - col * 8;
  const wh = h - row * 6.8;
  if (ww < 80 || wh < row * 3) return;
  roundRect(ctx, wx, wy, ww, wh, row * 0.3);
  ctx.fillStyle = rgba(RACK.window);
  ctx.fill();
  const [left, right] = channels(bandsOf(scene));
  const values = [
    ...resample(left, RACK_BANDS),
    ...resample(right, RACK_BANDS),
  ];
  state.peaks = updatePeaks(state.peaks, values, scene.now, scene.dt, 600, 0.6);
  const pitch = row * 0.42;
  const segments = Math.max(Math.floor((wh - row * 2.4) / pitch), 3);
  const half = ww / 2;
  const step = (half - col * 4) / RACK_BANDS;
  const ladder = Math.max(step * 0.62, 2);
  const base = wy + row * 0.8 + segments * pitch;
  const labelY = wy + wh - row * 0.8;
  for (let channel = 0; channel < 2; channel++) {
    const startX = wx + channel * half + col * 3;
    text(
      ctx,
      channel === 0 ? "L" : "R",
      startX - col * 1.8,
      wy + row * 0.9,
      RACK.label,
      px,
      { bold: true },
    );
    let edge = -Infinity;
    for (let b = 0; b < RACK_BANDS; b++) {
      const index = channel * RACK_BANDS + b;
      const x = startX + b * step;
      const lit = Math.round(clamp(values[index], 0, 1) * segments);
      for (let s = 0; s < segments; s++) {
        const share = s / segments;
        const led = RACK_LEDS[share < 0.64 ? 0 : share < 0.86 ? 1 : 2];
        ctx.fillStyle = s < lit ? led.lit : led.dim;
        ctx.fillRect(x, base - (s + 1) * pitch, ladder, pitch * 0.7);
      }
      // One dark segment above the peak, as on the TUI's rack.
      const peak = Math.min(
        Math.round(clamp(state.peaks[index]?.value ?? 0, 0, 1) * segments) + 1,
        segments - 1,
      );
      if (peak >= lit) {
        ctx.fillStyle = rgba(RACK.peak);
        ctx.fillRect(x, base - (peak + 1) * pitch, ladder, pitch * 0.35);
      }
      const label = RACK_LABELS[b];
      const halfLabel = textWidth(ctx, label, px * 0.75) / 2;
      const mid = x + ladder / 2;
      if (mid - halfLabel > edge + 2) {
        text(ctx, label, mid, labelY, RACK.label, px * 0.75, {
          align: "center",
        });
        edge = mid + halfLabel;
      }
    }
  }
  const knobY = h - row * 2.5;
  ["INPUT", "RANGE", "DECAY", "PEAK HOLD"].forEach((name, i) => {
    const x = w * (0.16 + i * 0.22);
    ctx.fillStyle = rgba(RACK.knob);
    ctx.beginPath();
    ctx.arc(x, knobY, row * 0.6, 0, Math.PI * 2);
    ctx.fill();
    ctx.strokeStyle = rgba(RACK.plate);
    ctx.lineWidth = 1.5;
    ctx.beginPath();
    ctx.moveTo(x, knobY);
    ctx.lineTo(
      x + row * 0.4 * Math.cos(-0.8 + i),
      knobY + row * 0.4 * Math.sin(-0.8 + i),
    );
    ctx.stroke();
    text(ctx, name, x + row, knobY, RACK.knobName, px * 0.8);
  });
}

// Cassette Deck: turning reels, LED level bars and a tape counter.

const TAPE = {
  body: rgb(34, 30, 28),
  shell: rgb(210, 200, 180),
  labelStrip: rgb(240, 235, 220),
  window: rgb(30, 26, 24),
  tape: rgb(90, 60, 40),
  hub: rgb(235, 230, 215),
  text: rgb(200, 180, 140),
  muted: rgb(150, 130, 100),
  counter: rgb(255, 120, 40),
  counterBg: rgb(10, 8, 6),
  ink: rgb(60, 50, 40),
  green: rgb(60, 220, 90),
  amber: rgb(250, 190, 40),
  red: rgb(250, 60, 40),
};
const TAPE_LEDS = ledColours(TAPE.body, [TAPE.green, TAPE.amber, TAPE.red]);
const TAPE_SCALE: [string, number][] = [
  ["-30", 0],
  ["-20", 0.25],
  ["-10", 0.5],
  ["-5", 0.65],
  ["0", 0.825],
  ["+3", 0.925],
  ["+6", 1],
];

export function paintCassette(scene: Scene): void {
  const { ctx, w, h, state, track } = scene;
  if (w < 160 || h < 100) return;
  const { row, col, px } = grid(h);
  ctx.fillStyle = rgba(TAPE.body);
  ctx.fillRect(0, 0, w, h);
  text(
    ctx,
    "AUTO REVERSE  ·  DOLBY B NR  ·  CrO₂",
    col * 3,
    row,
    TAPE.text,
    px,
  );
  text(ctx, "TAPE COUNTER", w - col * 18, row * 0.7, TAPE.muted, px * 0.8);
  const count = Math.floor((track.progressMs ?? 0) / 1000) % 10000;
  roundRect(ctx, w - col * 17, row * 1.3, col * 13, row * 1.3, row * 0.2);
  ctx.fillStyle = rgba(TAPE.counterBg);
  ctx.fill();
  text(
    ctx,
    [...String(count).padStart(4, "0")].join(" "),
    w - col * 10.5,
    row * 1.95,
    TAPE.counter,
    px * 1.1,
    {
      align: "center",
      bold: true,
    },
  );

  const progress = tapeProgress(track);
  const shellW = Math.max(w * 0.69, 200);
  const shellH = Math.max(Math.min(h / 2, h - row * 13), row * 6);
  const x = (w - shellW) / 2;
  const y = row * 3.4;
  roundRect(ctx, x, y, shellW, shellH, row * 0.8);
  ctx.fillStyle = rgba(TAPE.shell);
  ctx.fill();
  roundRect(
    ctx,
    x + col * 2,
    y + row * 0.6,
    shellW - col * 4,
    row * 2.2,
    row * 0.3,
  );
  ctx.fillStyle = rgba(TAPE.labelStrip);
  ctx.fill();
  text(
    ctx,
    `C-90   ${(track.title ?? "").toUpperCase()}   ·   SIDE A`,
    x + shellW / 2,
    y + row * 1.7,
    TAPE.ink,
    px,
    {
      align: "center",
    },
  );
  const winX = x + shellW * 0.12;
  const winY = y + row * 3.4;
  const winW = shellW * 0.76;
  const winH = Math.max(shellH - row * 4.2, row * 3);
  roundRect(ctx, winX, winY, winW, winH, row * 0.6);
  ctx.fillStyle = rgba(TAPE.window);
  ctx.fill();
  // The two full packs stay apart: their centres are 0.56 of the window apart.
  const full = Math.max(Math.min(winH / 2 - 4, winW * 0.2), 8);
  const hub = Math.max(full * 0.46, 6);
  const packs = tapePacks(progress, hub + 1, full);
  state.reels = stepReels(state.reels, packs, full, scene.dt, track.playing);
  const cy = winY + winH / 2;
  const centres = [winX + winW * 0.22, winX + winW * 0.78];
  ctx.strokeStyle = rgba(TAPE.tape);
  ctx.lineWidth = 2;
  ctx.beginPath();
  ctx.moveTo(centres[0], cy + packs[0]);
  ctx.lineTo(centres[1], cy + packs[1]);
  ctx.stroke();
  centres.forEach((cx, i) => {
    ctx.fillStyle = rgba(TAPE.tape);
    ctx.beginPath();
    ctx.arc(cx, cy, packs[i], 0, Math.PI * 2);
    ctx.fill();
    ctx.fillStyle = rgba(TAPE.window);
    ctx.beginPath();
    ctx.arc(cx, cy, hub + 0.5, 0, Math.PI * 2);
    ctx.fill();
    ctx.strokeStyle = rgba(TAPE.hub);
    ctx.lineWidth = Math.max(hub * 0.18, 1.5);
    ctx.beginPath();
    ctx.arc(cx, cy, hub, 0, Math.PI * 2);
    ctx.stroke();
    for (let tooth = 0; tooth < 6; tooth++) {
      const a = (state.reels[i] + tooth * 60) * DEG;
      ctx.beginPath();
      ctx.moveTo(cx + Math.cos(a) * hub * 0.55, cy + Math.sin(a) * hub * 0.55);
      ctx.lineTo(cx + Math.cos(a) * hub, cy + Math.sin(a) * hub);
      ctx.stroke();
    }
  });

  const [left, right] = levels(bandsOf(scene));
  const leds = clamp(Math.floor((w / col - 16) / 2), 8, 40);
  const x0 = col * 8;
  const span = w - col * 16;
  const pitch = span / leds;
  [left, right].forEach((level, channel) => {
    const ly = h - row * (7.5 - channel * 2);
    text(ctx, channel === 0 ? "L" : "R", col * 5, ly, TAPE.muted, px, {
      bold: true,
    });
    for (let k = 0; k < leds; k++) {
      const share = k / leds;
      const led = TAPE_LEDS[share < 0.65 ? 0 : share < 0.825 ? 1 : 2];
      ctx.fillStyle = share < level ? led.lit : led.dim;
      ctx.fillRect(x0 + k * pitch, ly - row * 0.4, pitch * 0.7, row * 0.8);
    }
  });
  for (const [label, share] of TAPE_SCALE)
    text(ctx, label, x0 + share * span, h - row * 3.6, TAPE.muted, px * 0.8, {
      align: "center",
    });
  text(
    ctx,
    track.playing ? "▶" : "‖",
    col * 6,
    h - row * 1.5,
    TAPE.text,
    px * 1.4,
    { bold: true },
  );
}
