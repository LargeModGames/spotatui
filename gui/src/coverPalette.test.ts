import { describe, expect, it } from "vitest";
import { DEFAULT_LOOK } from "../e2e/fixtures";
import {
  blendStops,
  fadeStops,
  legible,
  ordered,
  resolveStops,
  swatches,
  type Swatch,
} from "./coverPalette";
import { luminance, toHsv, toLinear, type Rgb } from "./visualizerModel";

/** RGBA pixels: each colour repeated its count of times. */
function pixels(...parts: [Rgb, number, number?][]): Uint8ClampedArray {
  return new Uint8ClampedArray(
    parts.flatMap(([[r, g, b], count, alpha = 255]) =>
      Array.from({ length: count }, () => [r, g, b, alpha]).flat(),
    ),
  );
}

const light = (colour: Rgb) => luminance(colour.map(toLinear) as Rgb);

const RED: Rgb = [200, 30, 30];
const BLUE: Rgb = [30, 60, 200];
const NEAR_RED: Rgb = [204, 34, 34];

describe("swatches", () => {
  it("finds a cover's colours, most common first, with their shares", () => {
    const found = swatches(pixels([BLUE, 300], [RED, 100]), DEFAULT_LOOK);
    expect(found.map((swatch) => swatch.colour)).toEqual([BLUE, RED]);
    expect(found.map((swatch) => swatch.share)).toEqual([0.75, 0.25]);
  });

  it("merges near-duplicates into one colour, and keeps them apart at a distance of 0", () => {
    const cover = pixels([RED, 300], [NEAR_RED, 200], [BLUE, 100]);
    const found = swatches(cover, DEFAULT_LOOK);
    expect(found.map((swatch) => swatch.colour)).toEqual([RED, BLUE]);
    expect(found[0].share).toBeCloseTo(5 / 6);
    const apart = swatches(cover, { ...DEFAULT_LOOK, min_distance: 0 });
    expect(apart.map((swatch) => swatch.colour)).toEqual([RED, NEAR_RED, BLUE]);
  });

  it("skips see-through pixels and specks under the least share", () => {
    const found = swatches(
      pixels([RED, 1000], [BLUE, 5], [[0, 255, 0], 500, 0]),
      DEFAULT_LOOK,
    );
    expect(found.map((swatch) => swatch.colour)).toEqual([RED]);
  });

  it("finds both halves of a grainy cover whose noise spreads each over many buckets", () => {
    const grain = (base: Rgb, count: number): [Rgb, number][] =>
      Array.from({ length: 64 }, (_, i) => [
        base.map(
          (c, channel) => c - 24 + 16 * ((i >> (2 * channel)) % 4),
        ) as Rgb,
        count,
      ]);
    const found = swatches(
      pixels(...grain([60, 110, 200], 32), ...grain([200, 120, 50], 32)),
      DEFAULT_LOOK,
    );
    expect(found.some(({ colour: [r, , b] }) => b > r + 60)).toBe(true);
    expect(found.some(({ colour: [r, , b] }) => r > b + 60)).toBe(true);
  });

  it("leaves out black, white and grey unless asked, or unless the cover has nothing else", () => {
    const BLACK: Rgb = [10, 10, 10];
    const cover = pixels([BLACK, 600], [RED, 300], [[245, 245, 245], 100]);
    expect(swatches(cover, DEFAULT_LOOK).map((s) => s.colour)).toEqual([RED]);
    expect(
      swatches(cover, { ...DEFAULT_LOOK, greys: true }).map((s) => s.colour),
    ).toEqual([BLACK, RED, [245, 245, 245]]);
    expect(
      swatches(pixels([BLACK, 10]), DEFAULT_LOOK).map((s) => s.colour),
    ).toEqual([BLACK]);
    expect(
      swatches(pixels([BLACK, 1000], [RED, 5]), DEFAULT_LOOK).map(
        (s) => s.colour,
      ),
    ).toEqual([BLACK]);
  });

  it("takes at most `count` colours and nothing from an empty cover", () => {
    const many = pixels(
      [RED, 100],
      [BLUE, 90],
      [[20, 200, 20], 80],
      [[230, 230, 30], 70],
    );
    expect(swatches(many, { ...DEFAULT_LOOK, count: 2 })).toHaveLength(2);
    expect(swatches(new Uint8ClampedArray(), DEFAULT_LOOK)).toEqual([]);
  });
});

describe("ordered", () => {
  const found: Swatch[] = [
    { colour: [240, 220, 40], share: 0.2 },
    { colour: [20, 20, 120], share: 0.5 },
    { colour: [128, 128, 128], share: 0.1 },
    { colour: [200, 30, 30], share: 0.2 },
  ];

  it("runs from dark to light by default and backwards when reversed", () => {
    const stops = ordered(found, DEFAULT_LOOK);
    const lights = stops.map(light);
    expect([...lights].sort((a, b) => a - b)).toEqual(lights);
    expect(ordered(found, { ...DEFAULT_LOOK, reverse: true })).toEqual(
      [...stops].reverse(),
    );
  });

  it("puts the largest first by share", () => {
    expect(ordered(found, { ...DEFAULT_LOOK, order: "share" })[0]).toEqual([
      20, 20, 120,
    ]);
  });

  it("walks the colour wheel from after its widest empty arc and puts the greys last", () => {
    const stops = ordered(found, { ...DEFAULT_LOOK, order: "hue" });
    expect(stops).toHaveLength(4);
    expect(stops.at(-1)).toEqual([128, 128, 128]);
    // Yellow to blue is the widest arc, so the walk starts at blue.
    expect(stops.slice(0, 3)).toEqual([
      [20, 20, 120],
      [200, 30, 30],
      [240, 220, 40],
    ]);
  });
});

describe("legible", () => {
  it("keeps white under the ceiling and lifts black to the floor", () => {
    expect(light(legible([255, 255, 255], DEFAULT_LOOK))).toBeLessThan(0.36);
    const black = legible([0, 0, 0], DEFAULT_LOOK);
    expect(light(black)).toBeGreaterThan(0.215);
    expect(black[0]).toBe(black[1]);
  });

  it("lifts a dark coloured stop to the floor without washing out its colour", () => {
    const dark: Rgb = [60, 15, 15];
    const lifted = legible(dark, DEFAULT_LOOK);
    expect(light(lifted)).toBeGreaterThan(0.215);
    const saturation = (colour: Rgb) => toHsv(colour.map(toLinear) as Rgb)[1];
    expect(saturation(lifted)).toBeCloseTo(saturation(dark), 2);
  });

  it("raises the saturation of a coloured stop and leaves a grey grey", () => {
    const look = { ...DEFAULT_LOOK, min_saturation: 0.9 };
    const [r, g, b] = legible([120, 100, 100], look);
    expect(r - g).toBeGreaterThan(20);
    expect(g).toBe(b);
    const [gr, gg, gb] = legible([100, 100, 100], look);
    expect(gr).toBe(gg);
    expect(gg).toBe(gb);
  });

  it("leaves a colour alone when the limits allow it", () => {
    const open = { ...DEFAULT_LOOK, luminance_floor: 0, luminance_ceiling: 1 };
    expect(legible([12, 200, 99], open)).toEqual([12, 200, 99]);
  });
});

describe("resolveStops", () => {
  const cover: Swatch[] = [
    { colour: BLUE, share: 0.5 },
    { colour: RED, share: 0.5 },
  ];

  it("uses the cover, then the fallback list, then the accent", () => {
    expect(resolveStops(DEFAULT_LOOK, cover, "#f0a83a")).toHaveLength(2);
    expect(
      resolveStops(
        { ...DEFAULT_LOOK, fallback: [[10, 200, 10]] },
        null,
        "#f0a83a",
      ),
    ).toHaveLength(1);
    expect(resolveStops(DEFAULT_LOOK, [], "#f0a83a")).toEqual([
      [211, 147, 50],
      [136, 172, 39],
    ]);
  });

  it("keeps the user's list in its own order and the accent source as before", () => {
    const list: Rgb[] = [
      [200, 30, 30],
      [30, 60, 200],
      [30, 200, 60],
    ];
    const look = { ...DEFAULT_LOOK, source: "list" as const, colors: list };
    expect(resolveStops(look, cover, "#f0a83a")).toHaveLength(3);
    expect(
      resolveStops({ ...DEFAULT_LOOK, source: "accent" }, cover, "#f0a83a"),
    ).toEqual([
      [211, 147, 50],
      [136, 172, 39],
    ]);
  });
});

describe("blendStops", () => {
  const two: Rgb[] = [
    [0, 0, 0],
    [200, 200, 200],
  ];
  const three: Rgb[] = [
    [200, 0, 0],
    [0, 200, 0],
    [0, 0, 200],
  ];

  it("starts at the old stops and ends at the new ones, whatever their counts", () => {
    expect(blendStops(two, three, 0, "srgb")).toEqual(two);
    expect(blendStops(two, three, 1, "srgb")).toEqual(three);
    const half = blendStops(two, three, 0.5, "srgb");
    expect(half).toHaveLength(3);
    expect(half[0]).toEqual([100, 0, 0]);
    expect(half[1]).toEqual([50, 150, 50]);
  });

  it("goes straight to the new stops when there were none", () => {
    expect(blendStops([], three, 0.2, "oklab")).toEqual(three);
  });
});

describe("fadeStops", () => {
  const fade = {
    from: [[0, 0, 0]] as Rgb[],
    to: [[200, 200, 200]] as Rgb[],
    start: 1000,
  };

  it("takes `fade_ms` to reach the new stops, or none at all", () => {
    expect(fadeStops(fade, 1400, DEFAULT_LOOK)).toEqual([[100, 100, 100]]);
    expect(fadeStops(fade, 1800, DEFAULT_LOOK)).toEqual([[200, 200, 200]]);
    expect(fadeStops(fade, 1000, { ...DEFAULT_LOOK, fade_ms: 0 })).toEqual([
      [200, 200, 200],
    ]);
  });
});
