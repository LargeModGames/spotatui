import { describe, expect, it } from "vitest";
import { freshState } from "./screenDraw";
import { barCount, fitBands, NO_TRACK, STYLES } from "./screenModel";
import { PAINTERS, turbo } from "./screenPaint";

/** A canvas context that takes every call and counts the drawing ones. */
function fakeContext() {
  const drawn: string[] = [];
  const gradient = { addColorStop: () => {} };
  const fields: Record<string | symbol, unknown> = {
    measureText: (value: string) => ({ width: value.length * 7 }),
    createLinearGradient: () => gradient,
    createRadialGradient: () => gradient,
  };
  const ctx = new Proxy(fields, {
    get: (target, key) =>
      key in target
        ? target[key]
        : (...args: unknown[]) => {
            drawn.push(String(key));
            return args.length;
          },
    set: (target, key, value) => {
      target[key] = value;
      return true;
    },
  });
  return { ctx: ctx as unknown as CanvasRenderingContext2D, drawn };
}

const music = (count: number) =>
  Array.from({ length: count }, (_, i) => 0.3 + 0.6 * Math.abs(Math.sin(i)));
const wave = Array.from({ length: 480 }, (_, i) => 0.6 * Math.sin(i * 0.11));

describe("painters", () => {
  it("draw every style with sound and in silence, from a large window down to a tiny one", () => {
    for (const style of STYLES)
      for (const [w, h] of [
        [1440, 800],
        [400, 200],
        [120, 60],
        [10, 5],
      ]) {
        const { ctx, drawn } = fakeContext();
        const state = freshState();
        const bars = barCount(style, w);
        for (const [frame, audio] of [
          { bands: music(bars), wave },
          { bands: fitBands([], bars), wave: [] },
        ].entries()) {
          PAINTERS[style]({
            ctx,
            w,
            h,
            audio,
            track: {
              ...NO_TRACK,
              title: "Heat Waves",
              progressMs: 61_000,
              durationMs: 238_000,
              playing: true,
            },
            now: 1000 + frame * 16,
            dt: frame === 0 ? 0 : 0.016,
            accent: [240, 168, 58],
            state,
          });
        }
        if (w >= 400)
          expect(drawn.length, `${style} at ${w}x${h}`).toBeGreaterThan(10);
      }
  });

  it("runs the bar graph gradient from blue low down to red at the top", () => {
    const [r0, , b0] = turbo(0.25);
    const [r1, , b1] = turbo(1);
    expect(b0).toBeGreaterThan(r0);
    expect(r1).toBeGreaterThan(b1);
  });
});
