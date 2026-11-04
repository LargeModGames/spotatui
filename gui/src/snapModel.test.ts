import { describe, expect, it } from "vitest";
import type { GuiLayout } from "./bindings/GuiLayout";
import { DEFAULT_LAYOUT, panes } from "./paneModel";
import { dockTargets, floatAt, hitTarget } from "./snapModel";

const AREA = { w: 1600, h: 800 };

describe("snapModel", () => {
  it("the area's edge wins over the pane edge under it", () => {
    const targets = dockTargets(DEFAULT_LAYOUT, "now_playing", AREA);
    expect(hitTarget(targets, 5, 400)?.id).toBe("area-left");
    expect(hitTarget(targets, 40, 400)?.id).toBe("main-left");
  });

  it("the zones are where the panes are now, the preview where the pane goes", () => {
    const targets = dockTargets(DEFAULT_LAYOUT, "now_playing", AREA);
    const top = hitTarget(targets, 600, 40)!;
    expect(top.id).toBe("main-top");
    expect(panes(top.result.tree)).toEqual(["now_playing", "main"]);
    expect(top.preview).toEqual({ x: 0, y: 0, w: 1600, h: 200 });
    expect(top.ok).toBe(true);
  });

  it("the middle of another pane floats, its centre swaps", () => {
    const targets = dockTargets(DEFAULT_LAYOUT, "now_playing", AREA);
    expect(hitTarget(targets, 300, 250)).toBeNull();
    const swap = hitTarget(targets, 600, 400)!;
    expect(swap.id).toBe("main-swap");
    expect(panes(swap.result.tree)).toEqual(["now_playing", "main"]);
  });

  it("the pane's own box cancels the drop", () => {
    const own = hitTarget(
      dockTargets(DEFAULT_LAYOUT, "now_playing", AREA),
      1400,
      400,
    );
    expect(own?.id).toBe("own");
    expect(own?.preview).toBeNull();
    expect(own?.result).toBe(DEFAULT_LAYOUT);
  });

  it("a floating pane has no swap box", () => {
    const floating: GuiLayout = {
      ...DEFAULT_LAYOUT,
      floating: [{ pane: "lyrics", x: 100, y: 100, width: 300, height: 300 }],
    };
    const targets = dockTargets(floating, "lyrics", AREA);
    expect(targets.some((target) => target.id.endsWith("swap"))).toBe(false);
    const docked = hitTarget(targets, 1590, 400)!;
    expect(docked.result.floating).toEqual([]);
    expect(panes(docked.result.tree)).toEqual([
      "main",
      "now_playing",
      "lyrics",
    ]);
  });

  it("a target that leaves a pane too small is refused", () => {
    const narrow = { w: 900, h: 800 };
    const targets = dockTargets(DEFAULT_LAYOUT, "now_playing", narrow);
    expect(hitTarget(targets, 5, 400)?.ok).toBe(false);
  });

  it("a float drop stays inside the area", () => {
    const floated = floatAt(
      DEFAULT_LAYOUT,
      "now_playing",
      { x: 1500, y: -50, w: 400, h: 300 },
      AREA,
    );
    expect(floated.tree).toBe("main");
    expect(floated.floating).toEqual([
      { pane: "now_playing", x: 750, y: 0, width: 250, height: 375 },
    ]);
  });
});
