import { describe, expect, it } from "vitest";
import type { BarPlacement } from "./bindings/BarPlacement";
import type { WindowEdge } from "./bindings/WindowEdge";
import { DEFAULT_BARS, edgeAt, frameGrid, moveBar } from "./barsModel";

const EDGES: WindowEdge[] = ["top", "bottom", "left", "right"];

describe("barsModel", () => {
  it("the default grid is the old frame", () => {
    expect(frameGrid(DEFAULT_BARS)).toEqual({
      areas: '"top" "panes" "player"',
      rows: "64px minmax(0, 1fr) 72px",
      columns: "minmax(0, 1fr)",
      padding: "0px 8px 0px",
    });
  });

  it("a player column on the left sits under a full-width top bar", () => {
    const bars = moveBar(DEFAULT_BARS, "player", "left");
    expect(frameGrid(bars)).toMatchObject({
      areas: '"top top" "player panes"',
      rows: "64px minmax(0, 1fr)",
      columns: "280px minmax(0, 1fr)",
    });
  });

  it("a bar dropped on an occupied edge goes inside the bar there", () => {
    const bars = moveBar(DEFAULT_BARS, "top", "bottom");
    expect(bars.map((bar) => bar.bar)).toEqual(["player", "top"]);
    expect(frameGrid(bars).areas).toBe('"panes" "top" "player"');
  });

  it("every pair of edges gives one rectangle per bar and one for the panes", () => {
    for (const top of EDGES)
      for (const player of EDGES) {
        const bars: BarPlacement[] = [
          { bar: "top", edge: top },
          { bar: "player", edge: player },
        ];
        const cells = frameGrid(bars)
          .areas.split('" "')
          .map((row) => row.replaceAll('"', "").split(" "));
        for (const name of ["top", "player", "panes"]) {
          const rows = cells.flatMap((row, r) =>
            row.includes(name) ? [r] : [],
          );
          const cols = cells.flatMap((row) =>
            row.flatMap((cell, c) => (cell === name ? [c] : [])),
          );
          const count = cells.flat().filter((cell) => cell === name).length;
          const height = Math.max(...rows) - Math.min(...rows) + 1;
          const width = Math.max(...cols) - Math.min(...cols) + 1;
          expect(count, `${name} with top ${top}, player ${player}`).toBe(
            height * width,
          );
        }
      }
  });

  it("edgeAt picks the nearest edge", () => {
    expect(edgeAt(10, 300, 1000, 800)).toBe("left");
    expect(edgeAt(500, 790, 1000, 800)).toBe("bottom");
    expect(edgeAt(980, 400, 1000, 800)).toBe("right");
    expect(edgeAt(500, 3, 1000, 800)).toBe("top");
  });
});
