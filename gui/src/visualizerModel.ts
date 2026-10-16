import type { VisualizerFrame } from "./bindings/VisualizerFrame";

export type VisualizerMode = "Off" | "Spectrum" | "Waveform";

export const VISUALIZER_SETTING = "behavior.player_bar_visualizer";

const MODES: VisualizerMode[] = ["Off", "Spectrum", "Waveform"];

/** A frame and when it arrived (a `performance.now()` value). */
export interface Frame {
  payload: VisualizerFrame;
  at: number;
}

/** The mode a click on empty bar space moves to. */
export function nextMode(mode: string): VisualizerMode {
  return MODES[(MODES.indexOf(mode as VisualizerMode) + 1) % MODES.length];
}

/** The bar's layout boxes: a click on one of them falls between the controls. */
const EMPTY_SPACE = [
  "playerbar",
  "now-track",
  "transport",
  "side",
  "buttons",
  "progress",
];

export function isEmptySpace(target: Element | null): boolean {
  return (
    target !== null &&
    EMPTY_SPACE.some((name) => target.classList.contains(name))
  );
}

/** A full bar falls in a third of a second. */
const FALL_PER_SECOND = 3;
/** A peak cap hangs this long, then drops under this gravity, in heights per second squared. */
const PEAK_HOLD = 0.35;
const PEAK_GRAVITY = 2.4;
/** Lower than this, a bar or a cap is at rest. */
const REST = 0.01;
/** A longer gap, such as a hidden tab, moves the bars as this one does. */
const LONGEST_STEP = 0.25;
/** A frame older than this no longer counts as sound. */
export const STALE_MS = 200;

export interface Bars {
  levels: number[];
  peaks: number[];
  /** How long each cap has hung, and how fast it falls. */
  held: number[];
  speed: number[];
}

export function restingBars(count: number): Bars {
  const zeros = () => Array<number>(count).fill(0);
  return { levels: zeros(), peaks: zeros(), held: zeros(), speed: zeros() };
}

/** Moves `bars` to `targets` (0 to 1, a missing one is silence) over `seconds`: up at once, down on the clock. */
export function stepBars(
  bars: Bars,
  targets: readonly number[],
  seconds: number,
): void {
  const step = Math.min(Math.max(seconds, 0), LONGEST_STEP);
  bars.levels.forEach((previous, band) => {
    const falling = Math.max(
      previous - FALL_PER_SECOND * step,
      targets[band] ?? 0,
    );
    const level = falling < REST ? 0 : falling;
    bars.levels[band] = level;
    if (level >= bars.peaks[band]) {
      bars.peaks[band] = level;
      bars.held[band] = 0;
      bars.speed[band] = 0;
    } else if (bars.held[band] < PEAK_HOLD) {
      bars.held[band] += step;
    } else {
      bars.speed[band] += PEAK_GRAVITY * step;
      bars.peaks[band] = Math.max(
        bars.peaks[band] - bars.speed[band] * step,
        level,
      );
    }
    if (bars.peaks[band] < REST) bars.peaks[band] = 0;
  });
}

export function settled(bars: Bars): boolean {
  return [...bars.levels, ...bars.peaks].every((level) => level === 0);
}

export type Rgb = [number, number, number];

export interface Colours {
  low: Rgb;
  high: Rgb;
}

/** The most luminance a colour may have under the white text over it. */
const LUMINANCE_CEILING = 0.35;
const LEAST_SATURATION = 0.55;

/**
 * The bass colour, the accent made vivid, and the treble colour, the same
 * turned a sixth of the way round the colour wheel: both dark enough for the
 * text over them. The steps are in linear light.
 */
export function barColours(accent: string): Colours {
  const [hue, saturation] = toHsv(parseHex(accent).map(toLinear) as Rgb);
  const vivid = (shift: number) =>
    underCeiling(
      fromHsv((hue + shift) % 1, Math.max(saturation, LEAST_SATURATION)),
    ).map(fromLinear) as Rgb;
  return { low: vivid(0), high: vivid(1 / 6) };
}

function parseHex(hex: string): Rgb {
  const value = /^#?([0-9a-f]{6})$/i.exec(hex.trim())?.[1] ?? "f0a83a";
  return [0, 2, 4].map((at) => parseInt(value.slice(at, at + 2), 16)) as Rgb;
}

function toLinear(channel: number): number {
  const c = channel / 255;
  return c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
}

function fromLinear(c: number): number {
  const encoded = c <= 0.0031308 ? c * 12.92 : 1.055 * c ** (1 / 2.4) - 0.055;
  return Math.round(Math.min(Math.max(encoded, 0), 1) * 255);
}

/** Hue and saturation, each from 0 to 1. */
function toHsv([r, g, b]: Rgb): [number, number] {
  const top = Math.max(r, g, b);
  const span = top - Math.min(r, g, b);
  if (span === 0) return [0, 0];
  const sector =
    top === r
      ? (g - b) / span
      : top === g
        ? 2 + (b - r) / span
        : 4 + (r - g) / span;
  return [(sector / 6 + 1) % 1, span / top];
}

/** The colour at full value. */
function fromHsv(hue: number, saturation: number): Rgb {
  const channel = (offset: number) => {
    const k = (offset + hue * 6) % 6;
    return 1 - saturation * Math.max(0, Math.min(k, 4 - k, 1));
  };
  return [channel(5), channel(3), channel(1)];
}

function underCeiling(rgb: Rgb): Rgb {
  const luminance = 0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2];
  const scale =
    luminance > LUMINANCE_CEILING ? LUMINANCE_CEILING / luminance : 1;
  return rgb.map((c) => c * scale) as Rgb;
}

/** `low` to `high` at `t`, channel by channel as stored. */
export function mix(low: Rgb, high: Rgb, t: number): Rgb {
  return low.map((c, i) => Math.round(c + (high[i] - c) * t)) as Rgb;
}

function rgba([r, g, b]: Rgb, alpha: number): string {
  return `rgb(${r} ${g} ${b} / ${alpha})`;
}

/** A rectangle shaded from `top` to `bottom`. */
export interface Shade {
  x: number;
  y: number;
  width: number;
  height: number;
  top: string;
  bottom: string;
}

const GAP = 2;
const CAP_HEIGHT = 2;
const CAP_GAP = 2;
/** How far the glow reaches past its bar, to the side and up and down. */
const GLOW_SIDE = GAP;
const GLOW_END = 3;
/** A bar fades from its foot to its top, with a glow round it and a brighter cap above. */
const FOOT_ALPHA = 0.28;
const TOP_ALPHA = 0.08;
const GLOW_ALPHA = 0.12;
const GLOW_TOP_ALPHA = GLOW_ALPHA * 0.3;
const CAP_ALPHA = 0.7;
/** The bass glows along the lower half of the bar, by the square of its level. */
const PULSE_ALPHA = 0.35;

/** The spectrum: bars from the foot of the bar, bass to treble, a cap above each, and a glow with the bass. */
export function spectrumShades(
  bars: Bars,
  width: number,
  height: number,
  { low, high }: Colours,
): Shade[] {
  const count = bars.levels.length;
  const bar = (width - GAP * (count - 1)) / count;
  const reach = height - CAP_GAP - CAP_HEIGHT - 1;
  const bassBands = Math.max(1, Math.floor(count / 8));
  const bass =
    bars.levels.slice(0, bassBands).reduce((sum, level) => sum + level, 0) /
    bassBands;
  const shades: Shade[] = [];
  if (bass > 0)
    shades.push({
      x: 0,
      y: height / 2,
      width,
      height: height / 2,
      top: rgba(low, 0),
      bottom: rgba(low, PULSE_ALPHA * bass * bass),
    });
  bars.levels.forEach((level, band) => {
    const x = band * (bar + GAP);
    const colour = mix(low, high, count > 1 ? band / (count - 1) : 0);
    const tall = level * reach;
    if (tall >= 1) {
      shades.push({
        x: x - GLOW_SIDE,
        y: height - tall - GLOW_END,
        width: bar + 2 * GLOW_SIDE,
        height: tall + 2 * GLOW_END,
        top: rgba(colour, GLOW_TOP_ALPHA),
        bottom: rgba(colour, GLOW_ALPHA),
      });
      shades.push({
        x,
        y: height - tall,
        width: bar,
        height: tall,
        top: rgba(colour, FOOT_ALPHA + (TOP_ALPHA - FOOT_ALPHA) * level),
        bottom: rgba(colour, FOOT_ALPHA),
      });
    }
    const cap = bars.peaks[band] * reach;
    if (cap >= CAP_HEIGHT) {
      const fill = rgba(colour, CAP_ALPHA);
      shades.push({
        x,
        y: height - cap - CAP_GAP - CAP_HEIGHT,
        width: bar,
        height: CAP_HEIGHT,
        top: fill,
        bottom: fill,
      });
    }
  });
  return shades;
}

/** The waveform's points for a bar `width` wide: about one each 8 px. */
export function wavePoints(width: number): number {
  return Math.min(320, Math.max(75, Math.floor(width / 8)));
}

/** Shading between the wave and the midline: its gradient runs from full at `peak` to clear at the midline. */
export interface WaveFill {
  corners: [number, number][];
  peak: number;
  midline: number;
  colour: string;
  /** The same colour, clear. */
  clear: string;
}

/** A line whose colour sweeps from `low` at the left edge to `high` at the right. */
export interface Line {
  points: [number, number][];
  width: number;
  low: string;
  high: string;
}

/** How far the wave swings, as a share of the bar's height. */
const WAVE_REACH = 0.46;
const WAVE_FILL_ALPHA = 0.14;
/** The line in layers, from the widest glow to the core, after a dim mirrored echo. */
const WAVE_LINES: [mirror: number, width: number, alpha: number][] = [
  [-0.6, 1, 0.18],
  [1, 9, 0.06],
  [1, 4, 0.16],
  [1, 1.6, 0.85],
];

/** The newest points of `wave` (-1 to 1, oldest first) that fit `width`, as a neon line over its shading. */
export function waveShapes(
  wave: readonly number[],
  width: number,
  height: number,
  { low, high }: Colours,
): { fills: WaveFill[]; lines: Line[] } {
  const trace = wave.slice(-wavePoints(width));
  if (trace.length < 2) return { fills: [], lines: [] };
  const last = trace.length - 1;
  const step = width / last;
  const midline = height / 2;
  const reach = WAVE_REACH * height;
  const point = (index: number, mirror = 1): [number, number] => [
    index * step,
    midline - trace[index] * mirror * reach,
  ];
  const fills = trace.slice(0, last).flatMap((_, index) => {
    const colour = mix(low, high, index / last);
    // Full at the swing's edge on the shape's side, so neighbouring shapes match.
    const fill = (corners: [number, number][], y: number): WaveFill => ({
      corners,
      peak: y < midline ? midline - reach : midline + reach,
      midline,
      colour: rgba(colour, WAVE_FILL_ALPHA),
      clear: rgba(colour, 0),
    });
    const [x0, y0] = point(index);
    const [x1, y1] = point(index + 1);
    const offset0 = y0 - midline;
    const offset1 = y1 - midline;
    if (offset0 * offset1 >= 0)
      return [
        fill(
          [
            [x0, y0],
            [x1, y1],
            [x1, midline],
            [x0, midline],
          ],
          (y0 + y1) / 2,
        ),
      ];
    // The wave crosses the midline here: one triangle on each side.
    const cross = x0 + ((x1 - x0) * offset0) / (offset0 - offset1);
    return [
      fill(
        [
          [x0, y0],
          [cross, midline],
          [x0, midline],
        ],
        y0,
      ),
      fill(
        [
          [cross, midline],
          [x1, y1],
          [x1, midline],
        ],
        y1,
      ),
    ];
  });
  const lines = WAVE_LINES.map(([mirror, lineWidth, alpha]) => ({
    points: trace.map((_, index) => point(index, mirror)),
    width: lineWidth,
    low: rgba(low, alpha),
    high: rgba(high, alpha),
  }));
  return { fills, lines };
}
