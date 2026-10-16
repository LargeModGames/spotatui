import { describe, expect, it } from "vitest";
import {
  barColours,
  isEmptySpace,
  nextMode,
  restingBars,
  settled,
  spectrumShades,
  stepBars,
  wavePoints,
  waveShapes,
  type Colours,
} from "./visualizerModel";

const element = (...names: string[]) =>
  ({
    classList: { contains: (name: string) => names.includes(name) },
  }) as unknown as Element;

const COLOURS: Colours = { low: [200, 100, 0], high: [0, 100, 200] };

describe("nextMode", () => {
  it("moves Off to Spectrum to Waveform and back to Off", () => {
    expect(nextMode("Off")).toBe("Spectrum");
    expect(nextMode("Spectrum")).toBe("Waveform");
    expect(nextMode("Waveform")).toBe("Off");
  });
});

describe("isEmptySpace", () => {
  it("takes the bar's layout boxes and not its controls or words", () => {
    expect(isEmptySpace(element("playerbar"))).toBe(true);
    expect(isEmptySpace(element("transport"))).toBe(true);
    expect(isEmptySpace(element("icon-button"))).toBe(false);
    expect(isEmptySpace(element())).toBe(false);
    expect(isEmptySpace(null)).toBe(false);
  });
});

describe("stepBars", () => {
  it("lifts a bar at once and lets it fall a full height in a third of a second", () => {
    const bars = restingBars(2);

    stepBars(bars, [1, 0.5], 0.016);
    expect(bars.levels).toEqual([1, 0.5]);

    stepBars(bars, [], 0.1);
    expect(bars.levels[0]).toBeCloseTo(0.7);
    expect(bars.levels[1]).toBeCloseTo(0.2);
    stepBars(bars, [], 0.25);
    expect(bars.levels).toEqual([0, 0]);
  });

  it("hangs a cap for 0.35 s, then drops it faster and faster, never below its bar", () => {
    const bars = restingBars(1);
    const step = 1 / 16;
    stepBars(bars, [1], step);

    for (let i = 0; i < 6; i++) stepBars(bars, [0.5], step);
    expect(bars.peaks[0]).toBe(1);

    stepBars(bars, [0.5], step);
    const first = 1 - bars.peaks[0];
    stepBars(bars, [0.5], step);
    const second = 1 - first - bars.peaks[0];
    expect(first).toBeGreaterThan(0);
    expect(second).toBeGreaterThan(first);

    for (let i = 0; i < 40; i++) stepBars(bars, [0.5], step);
    expect(bars.peaks[0]).toBe(0.5);
  });

  it("treats a long gap as a quarter second and comes to rest", () => {
    const bars = restingBars(1);
    stepBars(bars, [1], 0.016);

    stepBars(bars, [], 10);
    expect(bars.levels[0]).toBeCloseTo(0.25);
    expect(settled(bars)).toBe(false);

    for (let i = 0; i < 20; i++) stepBars(bars, [], 0.25);
    expect(settled(bars)).toBe(true);
  });
});

describe("barColours", () => {
  it("turns the amber accent into a vivid bass colour and a green treble colour", () => {
    expect(barColours("#f0a83a")).toEqual({
      low: [211, 147, 50],
      high: [136, 172, 39],
    });
  });

  it("keeps a light accent dark enough for white text", () => {
    const { low, high } = barColours(" #ffffaa ");
    for (const [r, g, b] of [low, high]) {
      const linear = [r, g, b].map((c) => (c / 255) ** 2.2);
      const luminance =
        0.2126 * linear[0] + 0.7152 * linear[1] + 0.0722 * linear[2];
      expect(luminance).toBeLessThan(0.37);
    }
  });
});

describe("spectrumShades", () => {
  it("draws a loud bass bar with its glow, its cap and a pulse along the lower half", () => {
    const bars = restingBars(8);
    bars.levels[0] = 1;
    bars.peaks[0] = 1;

    const shades = spectrumShades(bars, 30, 72, COLOURS);

    expect(shades).toEqual([
      {
        x: 0,
        y: 36,
        width: 30,
        height: 36,
        top: "rgb(200 100 0 / 0)",
        bottom: "rgb(200 100 0 / 0.35)",
      },
      {
        x: -2,
        y: 2,
        width: 6,
        height: 73,
        top: `rgb(200 100 0 / ${0.12 * 0.3})`,
        bottom: "rgb(200 100 0 / 0.12)",
      },
      {
        x: 0,
        y: 5,
        width: 2,
        height: 67,
        top: `rgb(200 100 0 / ${0.28 + (0.08 - 0.28)})`,
        bottom: "rgb(200 100 0 / 0.28)",
      },
      {
        x: 0,
        y: 1,
        width: 2,
        height: 2,
        top: "rgb(200 100 0 / 0.7)",
        bottom: "rgb(200 100 0 / 0.7)",
      },
    ]);
  });

  it("draws nothing for silence", () => {
    expect(spectrumShades(restingBars(75), 1440, 72, COLOURS)).toEqual([]);
  });

  it("sweeps the colour from bass to treble", () => {
    const bars = restingBars(3);
    bars.levels.fill(0.5);

    const feet = spectrumShades(bars, 30, 72, COLOURS)
      .filter((shade) => shade.bottom.endsWith("/ 0.28)"))
      .map((shade) => shade.bottom);

    expect(feet).toEqual([
      "rgb(200 100 0 / 0.28)",
      "rgb(100 100 100 / 0.28)",
      "rgb(0 100 200 / 0.28)",
    ]);
  });
});

describe("waveShapes", () => {
  it("fits one point for each 8 px, from 75 to 320", () => {
    expect(wavePoints(100)).toBe(75);
    expect(wavePoints(1440)).toBe(180);
    expect(wavePoints(4000)).toBe(320);
  });

  it("draws the newest points as an echo and three layers over a shading", () => {
    const wave = Array.from({ length: 320 }, (_, i) => (i < 240 ? 0.9 : 0.5));

    const { fills, lines } = waveShapes(wave, 600, 100, COLOURS);

    expect(fills).toHaveLength(74);
    expect(fills[0].corners).toHaveLength(4);
    expect(fills[0].peak).toBeCloseTo(4);
    expect(fills[0].midline).toBe(50);
    expect(fills[0].colour).toBe("rgb(200 100 0 / 0.14)");
    expect(fills[0].clear).toBe("rgb(200 100 0 / 0)");
    expect(lines.map((line) => line.width)).toEqual([1, 9, 4, 1.6]);
    expect(lines.every((line) => line.points.length === 75)).toBe(true);
    expect(lines[0].points[0][1]).toBeCloseTo(50 + 0.6 * 0.5 * 46);
    expect(lines[0].low).toBe("rgb(200 100 0 / 0.18)");
    expect(lines[0].high).toBe("rgb(0 100 200 / 0.18)");
  });

  it("splits the shading where the wave crosses the midline", () => {
    const { fills } = waveShapes([0.5, -0.5], 100, 100, COLOURS);
    const round = (value: number) => Math.round(value * 1000) / 1000;

    expect(
      fills.map(({ corners, peak }) => ({
        corners: corners.map(([x, y]) => [round(x), round(y)]),
        peak: round(peak),
      })),
    ).toEqual([
      {
        corners: [
          [0, 27],
          [50, 50],
          [0, 50],
        ],
        peak: 4,
      },
      {
        corners: [
          [50, 50],
          [100, 73],
          [100, 50],
        ],
        peak: 96,
      },
    ]);
  });

  it("draws nothing without a wave", () => {
    expect(waveShapes([], 600, 100, COLOURS)).toEqual({ fills: [], lines: [] });
  });
});
