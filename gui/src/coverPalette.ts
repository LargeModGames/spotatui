import type { LookInterpolation } from "./bindings/LookInterpolation";
import type { VisualizerLook } from "./bindings/VisualizerLook";
import {
  barColours,
  fromLinear,
  luminance,
  mixIn,
  sample,
  toHsv,
  toLinear,
  toOklab,
  type Rgb,
} from "./visualizerModel";

/** A colour of the cover and the part of the cover it stands for. */
export interface Swatch {
  colour: Rgb;
  share: number;
}

/** The cover is read at this size: enough for its colours, quick to scan. */
const SAMPLE_SIZE = 64;
/** Under this linear saturation a colour counts as grey and keeps its saturation. */
const GREY_SATURATION = 0.08;
/** Under this OKLab chroma a colour sorts with the greys. */
const GREY_CHROMA = 0.02;

/** The cover's pixels at 64 x 64 as RGBA; null when it cannot load or be read. */
export async function loadCoverPixels(
  src: string,
): Promise<Uint8ClampedArray | null> {
  const image = new Image();
  image.src = src;
  try {
    await image.decode();
    const canvas = document.createElement("canvas");
    canvas.width = SAMPLE_SIZE;
    canvas.height = SAMPLE_SIZE;
    const context = canvas.getContext("2d", { willReadFrequently: true });
    if (!context) return null;
    context.drawImage(image, 0, 0, SAMPLE_SIZE, SAMPLE_SIZE);
    return context.getImageData(0, 0, SAMPLE_SIZE, SAMPLE_SIZE).data;
  } catch {
    return null;
  }
}

/** The cover's main colours, largest first: groups of colours within `min_distance`, each but the first filling `min_share`, greys only when asked or when nothing else is there. */
export function swatches(
  pixels: Uint8ClampedArray,
  look: Pick<VisualizerLook, "count" | "min_distance" | "min_share" | "greys">,
): Swatch[] {
  const buckets = new Map<number, [number, number, number, number]>();
  let total = 0;
  for (let at = 0; at + 3 < pixels.length; at += 4) {
    if (pixels[at + 3] < 128) continue;
    const [r, g, b] = [pixels[at], pixels[at + 1], pixels[at + 2]];
    const key = ((r >> 4) << 8) | ((g >> 4) << 4) | (b >> 4);
    const bucket = buckets.get(key) ?? [0, 0, 0, 0];
    bucket[0] += 1;
    bucket[1] += r;
    bucket[2] += g;
    bucket[3] += b;
    buckets.set(key, bucket);
    total += 1;
  }
  const candidates = [...buckets.entries()]
    .map(([key, [count, r, g, b]]) => {
      const colour: Rgb = [r, g, b].map((sum) =>
        Math.round(sum / count),
      ) as Rgb;
      return { key, colour, lab: toOklab(colour), share: count / total };
    })
    .sort((a, b) => b.share - a.share || a.key - b.key);
  // A noisy region spreads over many small buckets: group them before any share counts.
  const groups: (Swatch & { lab: Rgb })[] = [];
  for (const { colour, lab, share } of candidates) {
    const near =
      look.min_distance > 0
        ? groups.find((group) => distance(group.lab, lab) < look.min_distance)
        : undefined;
    if (near) near.share += share;
    else groups.push({ colour, lab, share });
  }
  const coloured = groups.filter(
    ({ lab: [, a, b], share }) => !isGrey(a, b) && share >= look.min_share,
  );
  return (look.greys || coloured.length === 0 ? groups : coloured)
    .sort((a, b) => b.share - a.share)
    .filter((group, index) => index === 0 || group.share >= look.min_share)
    .slice(0, look.count)
    .map(({ colour, share }) => ({ colour, share }));
}

function isGrey(a: number, b: number): boolean {
  return Math.hypot(a, b) < GREY_CHROMA;
}

function distance(a: Rgb, b: Rgb): number {
  return Math.hypot(a[0] - b[0], a[1] - b[1], a[2] - b[2]);
}

/** The swatches as gradient stops, bass first. */
export function ordered(
  found: Swatch[],
  look: Pick<VisualizerLook, "order" | "reverse">,
): Rgb[] {
  const light = (colour: Rgb) => luminance(colour.map(toLinear) as Rgb);
  let stops: Rgb[];
  if (look.order === "share") {
    stops = [...found]
      .sort((a, b) => b.share - a.share)
      .map((swatch) => swatch.colour);
  } else if (look.order === "luminance") {
    stops = found
      .map((swatch) => swatch.colour)
      .sort((a, b) => light(a) - light(b));
  } else {
    stops = byHue(
      found.map((swatch) => swatch.colour),
      light,
    );
  }
  return look.reverse ? stops.reverse() : stops;
}

/** Round the colour wheel from just after its widest empty arc, so the sweep does not wrap; greys last, darkest first. */
function byHue(colours: Rgb[], light: (colour: Rgb) => number): Rgb[] {
  const hued: { colour: Rgb; hue: number }[] = [];
  const greys: Rgb[] = [];
  for (const colour of colours) {
    const [, a, b] = toOklab(colour);
    if (isGrey(a, b)) greys.push(colour);
    else hued.push({ colour, hue: (Math.atan2(b, a) / (2 * Math.PI) + 1) % 1 });
  }
  hued.sort((a, b) => a.hue - b.hue);
  let start = 0;
  let widest = -1;
  hued.forEach(({ hue }, index) => {
    const previous = hued[(index + hued.length - 1) % hued.length].hue;
    const gap = (hue - previous + 1) % 1 || 1;
    if (gap > widest) {
      widest = gap;
      start = index;
    }
  });
  return [...hued.slice(start), ...hued.slice(0, start)]
    .map(({ colour }) => colour)
    .concat(greys.sort((a, b) => light(a) - light(b)));
}

/** The colour with at least `min_saturation` unless grey, and its linear luminance between the floor and the ceiling. */
export function legible(
  colour: Rgb,
  look: Pick<
    VisualizerLook,
    "min_saturation" | "luminance_floor" | "luminance_ceiling"
  >,
): Rgb {
  let linear = colour.map(toLinear) as Rgb;
  const [, saturation] = toHsv(linear);
  if (saturation > GREY_SATURATION && saturation < look.min_saturation) {
    const top = Math.max(...linear);
    const stretch = look.min_saturation / saturation;
    linear = linear.map((c) => top - (top - c) * stretch) as Rgb;
  }
  let light = luminance(linear);
  if (light > look.luminance_ceiling)
    linear = linear.map((c) => (c * look.luminance_ceiling) / light) as Rgb;
  else if (light < look.luminance_floor) {
    // Scaling keeps the colour; white is mixed in only for what scaling cannot reach.
    const top = Math.max(...linear);
    if (top > 0) {
      const scale = Math.min(look.luminance_floor / light, 1 / top);
      linear = linear.map((c) => c * scale) as Rgb;
      light = luminance(linear);
    }
    if (light < look.luminance_floor) {
      const toward = (look.luminance_floor - light) / (1 - light);
      linear = linear.map((c) => c + (1 - c) * toward) as Rgb;
    }
  }
  return linear.map(fromLinear) as Rgb;
}

/** The gradient's stops: from the cover, from the user's list, or from the accent. */
export function resolveStops(
  look: VisualizerLook,
  cover: Swatch[] | null,
  accent: string,
): Rgb[] {
  const fit = (colours: Rgb[]) =>
    colours.map((colour) => legible(colour, look));
  const fromAccent = () => barColours(accent, look.luminance_ceiling);
  switch (look.source) {
    case "accent":
      return fromAccent();
    case "list":
      return fit(look.colors);
    case "cover":
      if (cover && cover.length > 0) return fit(ordered(cover, look));
      return look.fallback.length > 0 ? fit(look.fallback) : fromAccent();
  }
}

/** The stops `t` of the way from `from` to `to`; both are spread over as many stops as the longer one. */
export function blendStops(
  from: Rgb[],
  to: Rgb[],
  t: number,
  space: LookInterpolation,
): Rgb[] {
  if (t >= 1 || from.length === 0) return to;
  if (t <= 0) return from;
  const count = Math.max(from.length, to.length);
  const at = (stops: Rgb[], index: number) =>
    sample({ stops, space }, count > 1 ? index / (count - 1) : 0);
  return Array.from({ length: count }, (_, index) =>
    mixIn(space, at(from, index), at(to, index), t),
  );
}

/** A change of stops under way since `start` (a `performance.now()` value). */
export interface Fade {
  from: Rgb[];
  to: Rgb[];
  start: number;
}

/** The stops a fade shows at `now`. */
export function fadeStops(
  { from, to, start }: Fade,
  now: number,
  look: Pick<VisualizerLook, "fade_ms" | "interpolation">,
): Rgb[] {
  const t = look.fade_ms > 0 ? (now - start) / look.fade_ms : 1;
  return blendStops(from, to, t, look.interpolation);
}
