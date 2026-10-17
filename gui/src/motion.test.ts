import { afterEach, describe, expect, it, vi } from "vitest";
import { REDUCED_MOTION, scrollBehavior } from "./motion";

const osAsksForLessMotion = (reduce: boolean) =>
  vi.stubGlobal("window", {
    matchMedia: (query: string) => ({
      matches: reduce && query === REDUCED_MOTION,
    }),
  });

describe("scrollBehavior", () => {
  afterEach(() => vi.unstubAllGlobals());

  it("scrolls smoothly when the OS has no motion preference", () => {
    osAsksForLessMotion(false);
    expect(scrollBehavior()).toBe("smooth");
  });

  it("jumps when the OS asks for reduced motion", () => {
    osAsksForLessMotion(true);
    expect(scrollBehavior()).toBe("auto");
  });
});
