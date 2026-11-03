import type { ScreenFrame } from "./bindings/ScreenFrame";
import type { ScreenStrings } from "./bindings/ScreenStrings";
import type { VisualizerStyle } from "./bindings/VisualizerStyle";
import {
  type Frame,
  LONGEST_STEP,
  mix,
  rgba,
  type Rgb,
} from "./visualizerModel";

export type Style = VisualizerStyle;

/**
 * Every terminal style in the TUI's cycle order, with the screen string that
 * names it (Cava's name is the same in every language). A style added in Rust
 * fails the type check here.
 */
const NAMES: Record<Style, keyof ScreenStrings | null> = {
  Cava: null,
  BarGraph: "bar_graph",
  HeadUnit: "head_unit",
  AnalogVu: "analog_vu",
  Winamp: "winamp",
  PhosphorScope: "phosphor_scope",
  Outrun: "outrun",
  RackEq: "rack_eq",
  Cassette: "cassette_deck",
};
export const STYLES = Object.keys(NAMES) as Style[];

export function styleName(style: Style, screens: ScreenStrings): string {
  const key = NAMES[style];
  return key ? screens[key] : "Cava";
}

/** The page keeps its own choice; the TUI's `behavior.visualizer_style` is not touched. */
export const STYLE_KEY = "spotatui.visualizer_style";

export function readStyle(stored: string | null): Style {
  return STYLES.find((style) => style === stored) ?? "Cava";
}

export function saveStyle(storage: Pick<Storage, "setItem">, style: Style) {
  try {
    storage.setItem(STYLE_KEY, style);
  } catch {
    // A full or blocked storage only forgets the choice at the next load.
  }
}

export function nextStyle(style: Style): Style {
  return STYLES[(STYLES.indexOf(style) + 1) % STYLES.length];
}

/** One terminal cell in CSS pixels: the width rule of Cava and BarGraph counts cells. */
export const CELL_PX = 9;
const MAX_CAVA_BARS = 40;

/** The bands a style asks cava for, both channels together, as the TUI asks. */
export function barCount(style: Style, widthPx: number): number {
  const cells = Math.max(0, Math.floor(widthPx / CELL_PX));
  switch (style) {
    case "Cava":
      return clamp(Math.floor((cells + 1) / 3), 2, MAX_CAVA_BARS);
    case "BarGraph":
      return Math.max(cells * 2, 2);
    case "HeadUnit":
      return HEAD_UNIT_BANDS * 2;
    case "AnalogVu":
    case "Cassette":
      return 24;
    case "Winamp":
      return WINAMP_BARS * 2;
    // The TUI asks for 16 too; the scope draws only the wave.
    case "PhosphorScope":
      return 16;
    case "Outrun":
      return RIDGE_POINTS * 2;
    case "RackEq":
      return RACK_BANDS * 2;
  }
}

/** A frame as the styles read it: bands 0 to 1 in cava's mirrored order, the wave -1 to 1. */
export interface Audio {
  bands: number[];
  wave: number[];
}

export const SILENT: Audio = { bands: [], wave: [] };

/** Asks for frames at `bars` bands while the listener lives. */
export interface ScreenFeed {
  /** Hands frames to `listener` until the returned stop runs. */
  watch: (listener: (frame: Frame<ScreenFrame>) => void) => () => void;
  /** The band count the style draws; a new count goes out once, the watch stays. */
  bars: (count: number) => void;
}

export function audioOf(frame: ScreenFrame): Audio {
  return {
    bands: frame.bands.map((band) => band / 255),
    wave: frame.wave.map((value) => value / 127),
  };
}

/** Each channel bass to treble: the left half is stored reversed. */
export function channels(bands: number[]): [number[], number[]] {
  const half = Math.floor(bands.length / 2);
  const left = Array.from({ length: half }, (_, i) => bands[half - 1 - i]);
  return [left, bands.slice(half, half * 2)];
}

/** One bass-to-treble row, the louder channel per band. */
export function mono(bands: number[]): number[] {
  const [left, right] = channels(bands);
  return left.map((value, i) => Math.max(value, right[i] ?? 0));
}

/** Each channel's loudness: the RMS of its bands times 1.4, so music sits in the upper half. */
export function levels(bands: number[]): [number, number] {
  const [left, right] = channels(bands);
  return [rmsLevel(left), rmsLevel(right)];
}

function rmsLevel(values: number[]): number {
  if (values.length === 0) return 0;
  const mean = values.reduce((sum, x) => sum + x * x, 0) / values.length;
  return clamp(Math.sqrt(mean) * 1.4, 0, 1);
}

/** How fast held bands fall after the sound stops, in full scale a second. */
const FALL_PER_SEC = 1.5;

/** The bands to show: a new frame replaces them, silence lets them fall. */
export function settleBands(
  held: number[],
  bands: number[],
  dt: number,
): number[] {
  if (bands.length > 0) return bands;
  return held.some((value) => value > 0)
    ? held.map((value) => Math.max(0, value - FALL_PER_SEC * dt))
    : held;
}

/** Each channel resampled to the count this page asked for (another page can ask for another); no frame yet rests at zero. */
export function fitBands(bands: number[], bars: number): number[] {
  const half = Math.floor(bars / 2);
  if (bands.length === half * 2) return bands;
  const [left, right] = channels(bands);
  return [...resample(left, half).reverse(), ...resample(right, half)];
}

const FLAT = [0, 0];

/** What the painters read: bands at this page's count, and a flat line for a wave of silence. */
export function sceneAudio(
  held: number[],
  wave: number[],
  bars: number,
): Audio {
  return { bands: fitBands(held, bars), wave: wave.length > 1 ? wave : FLAT };
}

/** Linear resample onto `target` points. */
export function resample(values: number[], target: number): number[] {
  if (target <= 0) return [];
  if (values.length === 0) return new Array<number>(target).fill(0);
  if (values.length === 1 || target === 1)
    return new Array<number>(target).fill(values[0]);
  const scale = (values.length - 1) / (target - 1);
  return Array.from({ length: target }, (_, i) => {
    const pos = i * scale;
    const index = Math.floor(pos);
    const next = values[index + 1];
    if (next === undefined) return values[Math.min(index, values.length - 1)];
    const frac = pos - index;
    return values[index] * (1 - frac) + next * frac;
  });
}

export function clamp(value: number, low: number, high: number): number {
  return Math.min(high, Math.max(low, value));
}

/** Seconds since the last frame: none on the first, at most a quarter second. */
export function stepSeconds(last: number | null, now: number): number {
  return last === null ? 0 : clamp((now - last) / 1000, 0, LONGEST_STEP);
}

/** A falling marker: it holds, then drops at a steady rate. */
export interface Peak {
  value: number;
  heldAt: number;
}

/** A new high resets the hold; after `holdMs` the marker falls `fallPerSec` of full scale a second. */
export function updatePeaks(
  peaks: Peak[],
  values: number[],
  now: number,
  dt: number,
  holdMs: number,
  fallPerSec: number,
): Peak[] {
  const seeded =
    peaks.length === values.length
      ? peaks
      : values.map((value) => ({ value, heldAt: now }));
  return seeded.map((peak, i) => {
    const value = values[i];
    if (value >= peak.value) return { value, heldAt: now };
    if (now - peak.heldAt > holdMs)
      return { ...peak, value: Math.max(peak.value - fallPerSec * dt, value) };
    return peak;
  });
}

/** What the readouts of the faceplate styles show about the playing item. */
export interface TrackView {
  title: string | null;
  progressMs: number | null;
  durationMs: number | null;
  playing: boolean;
  shuffle: boolean;
  repeat: boolean;
  live: boolean;
}

export const NO_TRACK: TrackView = {
  title: null,
  progressMs: null,
  durationMs: null,
  playing: false,
  shuffle: false,
  repeat: false,
  live: false,
};

// Analog VU: a needle on a dB scale.

/** Where 0 VU sits on the arc, from 0 (left) to 1 (right). */
export const VU_ZERO = 0.72;
export const VU_MARKS: [number, number][] = [
  [-20, 0],
  [-10, 0.22],
  [-7, 0.36],
  [-5, 0.48],
  [-3, 0.6],
  [0, VU_ZERO],
  [2, 0.86],
  [3, 1],
];

/** The dB a needle position reads, linear between the scale's marks. */
export function vuDb(position: number): number {
  const at = clamp(position, 0, 1);
  for (let i = 1; i < VU_MARKS.length; i++) {
    const [db0, f0] = VU_MARKS[i - 1];
    const [db1, f1] = VU_MARKS[i];
    if (at <= f1) return db0 + ((at - f0) / (f1 - f0)) * (db1 - db0);
  }
  return 3;
}

/** The needle eases toward the level: up at 9 a second, down at 5. */
export function stepNeedle(needle: number, target: number, dt: number): number {
  const rate = target > needle ? 9 : 5;
  return needle + (target - needle) * Math.min(rate * dt, 1);
}

// Head Unit: a VFD spectrum in perspective with an eased level meter.

export const HEAD_UNIT_BANDS = 12;

const FRAME_SECONDS = 1 / 60;

/** The meter eases on time, not on redraws: 0.6 of the gap a 60 Hz frame up, 0.15 down. */
export function stepMeter(
  level: number,
  live: number[],
  dtSeconds: number,
): number {
  const target = rmsLevel(live);
  // A style's first frame has no step yet; it moves as one 60 Hz frame does.
  const dt = dtSeconds > 0 ? dtSeconds : FRAME_SECONDS;
  const perFrame = target > level ? 0.6 : 0.15;
  const ease = 1 - Math.pow(1 - perFrame, dt / FRAME_SECONDS);
  return level + (target - level) * ease;
}

export type Point = [number, number];

/** A square-to-quad projection; corners for (0,0), (1,0), (1,1), (0,1). */
export class Projection {
  private readonly m: number[][];

  constructor([p0, p1, p2, p3]: [Point, Point, Point, Point]) {
    const [dx1, dx2, dx3] = [
      p1[0] - p2[0],
      p3[0] - p2[0],
      p0[0] - p1[0] + p2[0] - p3[0],
    ];
    const [dy1, dy2, dy3] = [
      p1[1] - p2[1],
      p3[1] - p2[1],
      p0[1] - p1[1] + p2[1] - p3[1],
    ];
    let g = 0;
    let h = 0;
    if (Math.abs(dx3) >= 1e-9 || Math.abs(dy3) >= 1e-9) {
      const det = dx1 * dy2 - dx2 * dy1;
      g = (dx3 * dy2 - dx2 * dy3) / det;
      h = (dx1 * dy3 - dx3 * dy1) / det;
    }
    this.m = [
      [p1[0] - p0[0] + g * p1[0], p3[0] - p0[0] + h * p3[0], p0[0]],
      [p1[1] - p0[1] + g * p1[1], p3[1] - p0[1] + h * p3[1], p0[1]],
      [g, h, 1],
    ];
  }

  apply(u: number, v: number): Point {
    const m = this.m;
    const w = m[2][0] * u + m[2][1] * v + m[2][2];
    return [
      (m[0][0] * u + m[0][1] * v + m[0][2]) / w,
      (m[1][0] * u + m[1][1] * v + m[1][2]) / w,
    ];
  }
}

/** The floor's corners as fractions of the stage: front-left, front-right, back-right, back-left. */
export const FLOOR: [Point, Point, Point, Point] = [
  [0.1, 0.995],
  [0.95, 0.8],
  [0.8, 0.4],
  [0.04, 0.4],
];

export const VFD = {
  glass: [6, 8, 12] as Rgb,
  green: [58, 196, 104] as Rgb,
  lavender: [186, 156, 232] as Rgb,
  blue: [52, 104, 226] as Rgb,
  wallTop: [18, 70, 220] as Rgb,
  wallBottom: [70, 146, 232] as Rgb,
};

/** Each colour lit, and unlit as `dim` of the way from `base` to it, built once. */
export function ledColours(
  base: Rgb,
  colours: Rgb[],
  dim: number,
): { lit: string; dim: string }[] {
  return colours.map((colour) => ({
    lit: rgba(colour),
    dim: rgba(mix(base, colour, dim)),
  }));
}

const STRIP = ledColours(VFD.glass, [VFD.green, VFD.lavender, VFD.blue], 0.2);

/** A strip runs green, lavender, then blue from front to back, and dims past the level. */
export function stripColour(depth: number, level: number): string {
  const zone = STRIP[depth < 0.46 ? 0 : depth < 0.7 ? 1 : 2];
  return depth <= level ? zone.lit : zone.dim;
}

/** The frequency marks under the floor, on the 50 Hz to 10 kHz log scale. */
export const HEAD_UNIT_MARKS: [number, string][] = [
  [63, "63"],
  [160, "160"],
  [400, "400"],
  [1000, "1k"],
  [2500, "2.5k"],
  [6300, "6.3k"],
];

export function markBand(hz: number): number {
  return Math.round(
    (Math.log(hz / 50) / Math.log(200)) * (HEAD_UNIT_BANDS - 1),
  );
}

// Winamp: nineteen bars, as the classic player draws.

export const WINAMP_BARS = 19;

// Phosphor Scope: four sweeps that fade.

export const TRAILS = [0.18, 0.32, 0.55, 1];

/** A sweep joins the trails only when the wave changed, so a redraw cannot flood them. */
export function pushSweep(trails: number[][], wave: number[]): number[][] {
  const newest = trails[trails.length - 1];
  if (
    newest === wave ||
    (newest &&
      newest.length === wave.length &&
      newest.every((value, i) => value === wave[i]))
  )
    return trails;
  return [...trails, wave].slice(-TRAILS.length);
}

/** Peak-to-peak "volts" and a crossing count over the 50 ms window. */
export function scopeReadout(wave: number[]): { vpp: number; hz: number } {
  let [low, high, crossings] = [wave[0], wave[0], 0];
  for (let i = 1; i < wave.length; i++) {
    low = Math.min(low, wave[i]);
    high = Math.max(high, wave[i]);
    if (wave[i - 1] < 0 !== wave[i] < 0) crossings++;
  }
  return { vpp: (high - low) * 2, hz: crossings / 2 / 0.05 };
}

// Outrun: the grid scrolls while the song plays.

export const RIDGE_POINTS = 48;

export function stepOutrun(
  phase: number,
  dt: number,
  playing: boolean,
): number {
  return playing ? (phase + 0.55 * dt) % 1 : phase;
}

// Rack EQ: ten third-octave bands for each channel.

export const RACK_BANDS = 10;
const ISO: [number, string][] = [
  [63, "63"],
  [80, "80"],
  [100, "100"],
  [125, "125"],
  [160, "160"],
  [200, "200"],
  [250, "250"],
  [315, "315"],
  [400, "400"],
  [500, "500"],
  [630, "630"],
  [800, "800"],
  [1000, "1k"],
  [1250, "1.2k"],
  [1600, "1.6k"],
  [2000, "2k"],
  [2500, "2.5k"],
  [3150, "3.1k"],
  [4000, "4k"],
  [5000, "5k"],
  [6300, "6.3k"],
  [8000, "8k"],
  [10000, "10k"],
];

/** Each band's centre on the 50 Hz to 10 kHz log scale, snapped to the nearest standard frequency. */
export const RACK_LABELS = Array.from({ length: RACK_BANDS }, (_, i) => {
  const centre = 50 * Math.pow(200, (i + 0.5) / RACK_BANDS);
  let best = ISO[0];
  for (const entry of ISO)
    if (
      Math.abs(Math.log(entry[0]) - Math.log(centre)) <
      Math.abs(Math.log(best[0]) - Math.log(centre))
    )
      best = entry;
  return best[1];
});

// Cassette: two reels that trade tape as the song plays.

/** The song's share played; 0.3 when the length is not known, as on live radio. */
export function tapeProgress(track: TrackView): number {
  const { progressMs, durationMs } = track;
  if (progressMs === null || durationMs === null) return 0.3;
  return clamp(progressMs / durationMs, 0, 1);
}

/** The supply and take-up packs as radii between the hub and the full reel. */
export function tapePacks(
  progress: number,
  hub: number,
  full: number,
): [number, number] {
  const room = full - hub;
  return [hub + room * (1 - progress), hub + room * progress];
}

/** Each reel turns at 150 degrees a second at the full radius; a smaller pack turns faster. */
export function stepReels(
  angles: [number, number],
  packs: [number, number],
  full: number,
  dt: number,
  playing: boolean,
): [number, number] {
  if (!playing) return angles;
  return [0, 1].map(
    (i) => (angles[i] + (150 * full * dt) / Math.max(packs[i], 1e-3)) % 360,
  ) as [number, number];
}
