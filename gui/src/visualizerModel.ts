import type { LookInterpolation } from "./bindings/LookInterpolation";
import type { SpectrumLook } from "./bindings/SpectrumLook";
import type { VisualizerFrame } from "./bindings/VisualizerFrame";
import type { WaveformLook } from "./bindings/WaveformLook";

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
  look: SpectrumLook,
): void {
  const step = Math.min(Math.max(seconds, 0), LONGEST_STEP);
  bars.levels.forEach((previous, band) => {
    const falling = Math.max(
      previous - look.fall_per_second * step,
      targets[band] ?? 0,
    );
    const level = falling < REST ? 0 : falling;
    bars.levels[band] = level;
    if (level >= bars.peaks[band]) {
      bars.peaks[band] = level;
      bars.held[band] = 0;
      bars.speed[band] = 0;
    } else if (bars.held[band] < look.peak_hold) {
      bars.held[band] += step;
    } else {
      bars.speed[band] += look.peak_gravity * step;
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

/** Colours spread evenly from bass to treble, blended in `space`. */
export interface Gradient {
  stops: Rgb[];
  space: LookInterpolation;
}

const LEAST_SATURATION = 0.55;

/**
 * The bass colour, the accent made vivid, and the treble colour, the same
 * turned a sixth of the way round the colour wheel: both under `ceiling` for
 * the text over them. The steps are in linear light.
 */
export function barColours(accent: string, ceiling: number): Rgb[] {
  const [hue, saturation] = toHsv(parseHex(accent).map(toLinear) as Rgb);
  const vivid = (shift: number) =>
    underCeiling(
      fromHsv((hue + shift) % 1, Math.max(saturation, LEAST_SATURATION)),
      ceiling,
    ).map(fromLinear) as Rgb;
  return [vivid(0), vivid(1 / 6)];
}

function parseHex(hex: string): Rgb {
  const value = /^#?([0-9a-f]{6})$/i.exec(hex.trim())?.[1] ?? "f0a83a";
  return [0, 2, 4].map((at) => parseInt(value.slice(at, at + 2), 16)) as Rgb;
}

export function toLinear(channel: number): number {
  const c = channel / 255;
  return c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
}

export function fromLinear(c: number): number {
  const encoded = c <= 0.0031308 ? c * 12.92 : 1.055 * c ** (1 / 2.4) - 0.055;
  return Math.round(Math.min(Math.max(encoded, 0), 1) * 255);
}

/** Hue and saturation, each from 0 to 1. */
export function toHsv([r, g, b]: Rgb): [number, number] {
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

/** Of a colour in linear light. */
export function luminance([r, g, b]: Rgb): number {
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

function underCeiling(rgb: Rgb, ceiling: number): Rgb {
  const light = luminance(rgb);
  const scale = light > ceiling ? ceiling / light : 1;
  return rgb.map((c) => c * scale) as Rgb;
}

/** `low` to `high` at `t`, channel by channel as stored. */
export function mix(low: Rgb, high: Rgb, t: number): Rgb {
  return low.map((c, i) => Math.round(c + (high[i] - c) * t)) as Rgb;
}

/** From an sRGB colour. */
export function toOklab(rgb: Rgb): Rgb {
  const [r, g, b] = rgb.map(toLinear);
  const l = Math.cbrt(0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b);
  const m = Math.cbrt(0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b);
  const s = Math.cbrt(0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b);
  return [
    0.2104542553 * l + 0.793617785 * m - 0.0040720468 * s,
    1.9779984951 * l - 2.428592205 * m + 0.4505937099 * s,
    0.0259040371 * l + 0.7827717662 * m - 0.808675766 * s,
  ];
}

function fromOklab([lightness, a, b]: Rgb): Rgb {
  const l = (lightness + 0.3963377774 * a + 0.2158037573 * b) ** 3;
  const m = (lightness - 0.1055613458 * a - 0.0638541728 * b) ** 3;
  const s = (lightness - 0.0894841775 * a - 1.291485548 * b) ** 3;
  return [
    4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s,
    -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s,
    -0.0041960863 * l - 0.7034186147 * m + 1.707614701 * s,
  ].map(fromLinear) as Rgb;
}

/** `low` to `high` at `t`, blended in `space`. */
export function mixIn(
  space: LookInterpolation,
  low: Rgb,
  high: Rgb,
  t: number,
): Rgb {
  const lerp = (a: Rgb, b: Rgb) => a.map((c, i) => c + (b[i] - c) * t) as Rgb;
  switch (space) {
    case "srgb":
      return mix(low, high, t);
    case "linear":
      return lerp(low.map(toLinear) as Rgb, high.map(toLinear) as Rgb).map(
        fromLinear,
      ) as Rgb;
    case "oklab":
      return fromOklab(lerp(toOklab(low), toOklab(high)));
  }
}

/** The gradient's colour at `t`, from 0 at the first stop to 1 at the last. */
export function sample({ stops, space }: Gradient, t: number): Rgb {
  if (stops.length < 2) return stops[0] ?? [0, 0, 0];
  const at = Math.min(Math.max(t, 0), 1) * (stops.length - 1);
  const index = Math.min(Math.floor(at), stops.length - 2);
  return mixIn(space, stops[index], stops[index + 1], at - index);
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

/** The spectrum: bars from the foot of the bar, bass to treble, a cap above each, and a glow with the bass. */
export function spectrumShades(
  bars: Bars,
  width: number,
  height: number,
  gradient: Gradient,
  look: SpectrumLook,
): Shade[] {
  const count = bars.levels.length;
  const bar = (width - look.gap * (count - 1)) / count;
  const reach = height - look.cap_gap - look.cap_height - 1;
  const bassBands = Math.max(1, Math.floor(count * look.pulse_bands));
  const bass =
    bars.levels.slice(0, bassBands).reduce((sum, level) => sum + level, 0) /
    bassBands;
  const low = sample(gradient, 0);
  const shades: Shade[] = [];
  if (bass > 0 && look.pulse_alpha > 0 && look.pulse_height > 0)
    shades.push({
      x: 0,
      y: height * (1 - look.pulse_height),
      width,
      height: height * look.pulse_height,
      top: rgba(low, 0),
      bottom: rgba(low, look.pulse_alpha * bass * bass),
    });
  bars.levels.forEach((level, band) => {
    const x = band * (bar + look.gap);
    const colour = sample(gradient, count > 1 ? band / (count - 1) : 0);
    const tall = level * reach;
    if (tall >= 1) {
      if (look.glow_alpha > 0)
        shades.push({
          x: x - look.glow_side,
          y: height - tall - look.glow_end,
          width: bar + 2 * look.glow_side,
          height: tall + 2 * look.glow_end,
          top: rgba(colour, look.glow_top_alpha),
          bottom: rgba(colour, look.glow_alpha),
        });
      shades.push({
        x,
        y: height - tall,
        width: bar,
        height: tall,
        top: rgba(
          colour,
          look.foot_alpha + (look.top_alpha - look.foot_alpha) * level,
        ),
        bottom: rgba(colour, look.foot_alpha),
      });
    }
    const cap = bars.peaks[band] * reach;
    if (look.cap_height > 0 && cap >= look.cap_height) {
      const fill = rgba(colour, look.cap_alpha);
      shades.push({
        x,
        y: height - cap - look.cap_gap - look.cap_height,
        width: bar,
        height: look.cap_height,
        top: fill,
        bottom: fill,
      });
    }
  });
  return shades;
}

/** The waveform's points for a bar `width` wide: one each `spacing` px, at least `least` and at most the server's 320. */
export function wavePoints(
  width: number,
  spacing: number,
  least: number,
): number {
  return Math.min(320, Math.max(least, Math.floor(width / spacing)));
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

/** A line whose colour sweeps through `colours`, spread evenly from the left edge to the right. */
export interface Line {
  points: [number, number][];
  width: number;
  colours: string[];
}

/** Samples a line's sweep takes between two stops. */
const LINE_STEPS = 4;

/** The newest points of `wave` (-1 to 1, oldest first) that fit `width`, as a neon line over its shading. */
export function waveShapes(
  wave: readonly number[],
  width: number,
  height: number,
  gradient: Gradient,
  look: WaveformLook,
): { fills: WaveFill[]; lines: Line[] } {
  const trace = wave.slice(
    -wavePoints(width, look.point_spacing, look.min_points),
  );
  if (trace.length < 2) return { fills: [], lines: [] };
  const last = trace.length - 1;
  const step = width / last;
  const midline = height / 2;
  const reach = look.reach * height;
  const point = (index: number, mirror = 1): [number, number] => [
    index * step,
    midline - trace[index] * mirror * reach,
  ];
  const fills =
    look.fill_alpha > 0
      ? trace.slice(0, last).flatMap((_, index) => {
          const colour = sample(gradient, index / last);
          // Full at the swing's edge on the shape's side, so neighbouring shapes match.
          const fill = (corners: [number, number][], y: number): WaveFill => ({
            corners,
            peak: y < midline ? midline - reach : midline + reach,
            midline,
            colour: rgba(colour, look.fill_alpha),
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
        })
      : [];
  const samples = Math.max(1, (gradient.stops.length - 1) * LINE_STEPS);
  const sweep = Array.from({ length: samples + 1 }, (_, index) =>
    sample(gradient, index / samples),
  );
  const lines = look.lines.map(([mirror, lineWidth, alpha]) => ({
    points: trace.map((_, index) => point(index, mirror)),
    width: lineWidth,
    colours: sweep.map((colour) => rgba(colour, alpha)),
  }));
  return { fills, lines };
}
