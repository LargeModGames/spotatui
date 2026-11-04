import { describe, expect, it } from "vitest";
import type { GuiLayout } from "./bindings/GuiLayout";
import type { PaneNode } from "./bindings/PaneNode";
import {
  close,
  closeQueue,
  DEFAULT_LAYOUT,
  dock,
  fits,
  float,
  layoutRects,
  neighbor,
  open,
  panes,
  setSplitPercent,
  showMain,
  snapPercent,
  swap,
  toggle,
} from "./paneModel";

const lyricsRight: PaneNode = {
  axis: "horizontal",
  percent: 75,
  first: "main",
  second: "lyrics",
};

function integers(value: unknown): boolean {
  if (typeof value === "number") return Number.isInteger(value);
  if (value && typeof value === "object")
    return Object.values(value).every(integers);
  return true;
}

describe("paneModel", () => {
  it("the default layout is Main beside Now Playing", () => {
    expect(panes(DEFAULT_LAYOUT.tree)).toEqual(["main", "now_playing"]);
    expect(DEFAULT_LAYOUT.bars.map((bar) => bar.edge)).toEqual([
      "top",
      "bottom",
    ]);
  });

  it("dock splits the target leaf on each side with the share on the new pane", () => {
    expect(dock("main", "lyrics", "main", "right", 25)).toEqual(lyricsRight);
    expect(dock("main", "lyrics", "main", "top", 30)).toEqual({
      axis: "vertical",
      percent: 30,
      first: "lyrics",
      second: "main",
    });
  });

  it("close hands the parent slot to the sibling", () => {
    expect(close(lyricsRight, "lyrics")).toBe("main");
    expect(close(lyricsRight, "queue")).toBe(lyricsRight);
  });

  it("Main hides beside another pane and comes back on the left", () => {
    expect(close(lyricsRight, "main")).toBe("lyrics");
    expect(close("main", "main")).toBe("main");
    expect(close("lyrics", "lyrics")).toBe("main");
    expect(showMain("lyrics")).toEqual({
      axis: "horizontal",
      percent: 60,
      first: "main",
      second: "lyrics",
    });
    expect(showMain(lyricsRight)).toBe(lyricsRight);
  });

  it("swap trades two leaves", () => {
    expect(panes(swap(lyricsRight, "main", "lyrics"))).toEqual([
      "lyrics",
      "main",
    ]);
  });

  it("a split share is clamped and rounded", () => {
    expect(setSplitPercent(lyricsRight, 0, 95.4)).toMatchObject({
      percent: 90,
    });
    expect(setSplitPercent(lyricsRight, 0, 60.4)).toMatchObject({
      percent: 60,
    });
    expect(setSplitPercent(lyricsRight, 0, 75)).toBe(lyricsRight);
  });

  it("a dragged share snaps near 25, 33, 50, 67 and 75", () => {
    expect(snapPercent(48.5)).toBe(50);
    expect(snapPercent(31.2)).toBe(33);
    expect(snapPercent(40.4)).toBe(40);
    expect(snapPercent(3)).toBe(10);
  });

  it("float takes the pane out of the tree and saves whole thousandths", () => {
    const floated = float({ ...DEFAULT_LAYOUT, tree: lyricsRight }, "lyrics", {
      x: 0.12345,
      y: 0.5,
      w: 0.3333,
      h: 0.25,
    });
    expect(floated.tree).toBe("main");
    expect(floated.floating).toEqual([
      { pane: "lyrics", x: 123, y: 500, width: 333, height: 250 },
    ]);
    expect(integers(floated)).toBe(true);
  });

  it("the queue takes Now Playing's slot and gives it back, as the right panel did", () => {
    const queued = toggle(DEFAULT_LAYOUT, "queue");
    expect(panes(queued.tree)).toEqual(["main", "queue"]);
    expect(closeQueue(queued)).toEqual(DEFAULT_LAYOUT);
    expect(toggle(DEFAULT_LAYOUT, "now_playing").tree).toBe("main");
  });

  it("open adds a pane on the right and leaves an open pane alone", () => {
    const opened = open(DEFAULT_LAYOUT, "lyrics");
    expect(panes(opened.tree)).toEqual(["main", "now_playing", "lyrics"]);
    expect(open(opened, "lyrics")).toBe(opened);
  });

  it("the queue closes alone when Now Playing is open elsewhere", () => {
    const both: GuiLayout = {
      ...DEFAULT_LAYOUT,
      tree: dock(DEFAULT_LAYOUT.tree, "queue", "now_playing", "bottom", 50),
    };
    expect(closeQueue(both)).toEqual(DEFAULT_LAYOUT);
  });

  it("the boxes tile the whole area", () => {
    const tree = dock(lyricsRight, "queue", "lyrics", "bottom", 50);
    const { rects, dividers } = layoutRects(tree);
    const area = [...rects.values()].reduce((sum, r) => sum + r.w * r.h, 0);
    expect(area).toBeCloseTo(1);
    expect(dividers).toHaveLength(2);
    expect(rects.get("queue")).toEqual({ x: 0.75, y: 0.5, w: 0.25, h: 0.5 });
  });

  it("fits refuses a pane under 240px and Main under 560px", () => {
    expect(fits(lyricsRight, 1200, 800)).toBe(true);
    expect(fits(lyricsRight, 900, 800)).toBe(false);
    expect(fits({ ...lyricsRight, percent: 40 }, 1200, 800)).toBe(false);
  });

  it("neighbor finds the pane in a direction", () => {
    const tree = dock(lyricsRight, "queue", "lyrics", "bottom", 50);
    const { rects } = layoutRects(tree);
    expect(neighbor(rects, "main", "right")).toBe("lyrics");
    expect(neighbor(rects, "lyrics", "bottom")).toBe("queue");
    expect(neighbor(rects, "queue", "left")).toBe("main");
    expect(neighbor(rects, "main", "left")).toBeNull();
  });
});
