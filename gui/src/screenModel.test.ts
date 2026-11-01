import { describe, expect, it } from "vitest";
import {
  barCount,
  CELL_PX,
  channels,
  fitBands,
  FLOOR,
  levels,
  mono,
  nextStyle,
  Projection,
  pushSweep,
  RACK_LABELS,
  readStyle,
  resample,
  scopeReadout,
  settleBands,
  stepMeter,
  stepNeedle,
  stepOutrun,
  stepReels,
  stripColour,
  tapePacks,
  tapeProgress,
  NO_TRACK,
  updatePeaks,
  VFD,
  VU_ZERO,
  vuDb,
} from "./screenModel";
import { rgba } from "./visualizerModel";

const MIRRORED = [0.1, 0.2, 0.9, 0.3, 0.4, 0.05];

describe("styles", () => {
  it("reads an unknown stored style as Cava and cycles back to it after Cassette", () => {
    expect(readStyle(null)).toBe("Cava");
    expect(readStyle("Equalizer")).toBe("Cava");
    expect(readStyle("Outrun")).toBe("Outrun");
    expect(nextStyle("Cava")).toBe("BarGraph");
    expect(nextStyle("Cassette")).toBe("Cava");
  });

  it("asks cava for the band counts the terminal asks for", () => {
    const cells = (count: number) => count * CELL_PX;
    expect(barCount("Cava", cells(80))).toBe(27);
    expect(barCount("Cava", 0)).toBe(2);
    expect(barCount("Cava", cells(4000))).toBe(40);
    expect(barCount("BarGraph", cells(100))).toBe(200);
    expect(barCount("BarGraph", 0)).toBe(2);
    expect(barCount("HeadUnit", cells(100))).toBe(24);
    expect(barCount("Winamp", cells(100))).toBe(38);
    expect(barCount("PhosphorScope", cells(100))).toBe(16);
    expect(barCount("Outrun", cells(100))).toBe(96);
    expect(barCount("RackEq", cells(100))).toBe(20);
    expect(barCount("Cassette", cells(100))).toBe(24);
  });
});

describe("audio", () => {
  it("un-mirrors the channels bass to treble and keeps the louder one in the mono row", () => {
    expect(channels(MIRRORED)).toEqual([
      [0.9, 0.2, 0.1],
      [0.3, 0.4, 0.05],
    ]);
    expect(mono(MIRRORED)).toEqual([0.9, 0.4, 0.1]);
  });

  it("measures each channel as the RMS of its bands times 1.4, at most full", () => {
    const [left, right] = levels([0.5, 0.5, 1, 1]);
    expect(left).toBeCloseTo(0.7);
    expect(right).toBe(1);
    expect(levels([])).toEqual([0, 0]);
  });

  it("lets held bands fall after the sound stops and takes a new frame at once", () => {
    const held = settleBands([], [0.8, 0.4], 0.016);
    expect(held).toEqual([0.8, 0.4]);
    const falling = settleBands(held, [], 0.2);
    expect(falling[0]).toBeCloseTo(0.5);
    expect(falling[1]).toBeCloseTo(0.1);
    expect(settleBands(falling, [], 1)).toEqual([0, 0]);
    expect(settleBands(falling, [0.9, 0.9], 0.1)).toEqual([0.9, 0.9]);
  });

  it("fits a frame asked for by another page to this page's band count", () => {
    expect(fitBands([0.2, 0.9, 0.9, 0.2], 4)).toEqual([0.2, 0.9, 0.9, 0.2]);
    expect(fitBands([0.2, 0.9, 0.9, 0.2], 27)).toHaveLength(26);
    const fitted = fitBands([0.0, 1.0, 1.0, 0.0], 6);
    expect(channels(fitted)).toEqual([
      [1, 0.5, 0],
      [1, 0.5, 0],
    ]);
    expect(fitBands([], 40)).toEqual([]);
  });

  it("resamples linearly and keeps both ends", () => {
    expect(resample([0, 1], 5)).toEqual([0, 0.25, 0.5, 0.75, 1]);
    expect(resample([], 3)).toEqual([0, 0, 0]);
    expect(resample([0.4], 2)).toEqual([0.4, 0.4]);
  });

  it("holds a peak, then lets it fall at its rate", () => {
    let peaks = updatePeaks([], [1], 0, 0, 300, 0.7);
    peaks = updatePeaks(peaks, [0], 200, 0.1, 300, 0.7);
    expect(peaks[0].value).toBe(1);
    peaks = updatePeaks(peaks, [0], 400, 0.1, 300, 0.7);
    expect(peaks[0].value).toBeCloseTo(0.93);
    peaks = updatePeaks(peaks, [0.95], 450, 0.05, 300, 0.7);
    expect(peaks[0]).toEqual({ value: 0.95, heldAt: 450 });
  });
});

describe("analog VU", () => {
  it("reads the dB scale between its marks", () => {
    expect(vuDb(0)).toBe(-20);
    expect(vuDb(VU_ZERO)).toBe(0);
    expect(vuDb(1)).toBe(3);
    expect(vuDb(0.11)).toBeCloseTo(-15);
  });

  it("swings a needle up faster than it falls back", () => {
    const up = stepNeedle(0, 1, 0.05);
    const down = 1 - stepNeedle(1, 0, 0.05);
    expect(up).toBeCloseTo(0.45);
    expect(down).toBeCloseTo(0.25);
  });
});

describe("head unit", () => {
  it("eases the meter on time, not on redraws", () => {
    const once = stepMeter(0, [0.9], 1 / 60);
    let twice = stepMeter(0, [0.9], 1 / 120);
    twice = stepMeter(twice, [0.9], 1 / 120);
    expect(once).toBeCloseTo(0.6 * Math.min(0.9 * 1.4, 1));
    expect(twice).toBeCloseTo(once);
    expect(stepMeter(0, [0.9], null)).toBeCloseTo(once);
  });

  it("maps the square onto the floor corners", () => {
    const floor = new Projection(FLOOR);
    const corners: [number, number][] = [
      [0, 0],
      [1, 0],
      [1, 1],
      [0, 1],
    ];
    corners.forEach(([u, v], i) => {
      const [x, y] = floor.apply(u, v);
      expect(x).toBeCloseTo(FLOOR[i][0], 4);
      expect(y).toBeCloseTo(FLOOR[i][1], 4);
    });
  });

  it("runs a strip green, lavender, then blue, and dims it past the level", () => {
    expect(stripColour(0.2, 1)).toBe(rgba(VFD.green));
    expect(stripColour(0.5, 1)).toBe(rgba(VFD.lavender));
    expect(stripColour(0.9, 1)).toBe(rgba(VFD.blue));
    expect(stripColour(0.5, 0.3)).not.toBe(rgba(VFD.lavender));
  });
});

describe("retro styles", () => {
  it("adds a sweep only for a new wave and keeps four", () => {
    let trails = pushSweep([], [0.1, 0.2]);
    trails = pushSweep(trails, [0.1, 0.2]);
    expect(trails).toHaveLength(1);
    for (let i = 0; i < 6; i++) trails = pushSweep(trails, [i, 0]);
    expect(trails).toHaveLength(4);
    expect(trails[3]).toEqual([5, 0]);
  });

  it("reads a sine's swing and its crossings over 50 ms", () => {
    const wave = Array.from({ length: 480 }, (_, i) =>
      Math.sin((i / 480) * Math.PI * 2 * 10 - 0.1),
    );
    const { vpp, hz } = scopeReadout(wave);
    expect(vpp).toBeCloseTo(4, 1);
    expect(hz).toBe(200);
  });

  it("scrolls the grid only while the song plays", () => {
    expect(stepOutrun(0.9, 0.5, true)).toBeCloseTo(0.175);
    expect(stepOutrun(0.9, 0.5, false)).toBe(0.9);
  });

  it("labels the rack bands with rising standard frequencies", () => {
    const labels = RACK_LABELS;
    expect(labels[0]).toBe("63");
    expect(labels[9]).toBe("8k");
    expect(new Set(labels).size).toBe(10);
  });

  it("trades tape between the reels and spins the smaller pack faster", () => {
    expect(tapeProgress(NO_TRACK)).toBe(0.3);
    expect(
      tapeProgress({ ...NO_TRACK, progressMs: 30_000, durationMs: 120_000 }),
    ).toBe(0.25);
    const packs = tapePacks(0.25, 10, 30);
    expect(packs).toEqual([25, 15]);
    const [left, right] = stepReels([0, 0], packs, 30, 0.1, true);
    expect(right).toBeGreaterThan(left);
    expect(stepReels([10, 20], packs, 30, 0.1, false)).toEqual([10, 20]);
  });
});
