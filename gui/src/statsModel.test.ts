import { describe, expect, it } from "vitest";
import type { StatsRow } from "./bindings/StatsRow";
import {
  cyclesTo,
  groupThousands,
  moveText,
  playable,
  sentence,
} from "./statsModel";

const artist = (all_time_rank: number | null, isNew = false): StatsRow => ({
  title: "Kygo",
  artist: null,
  uri: null,
  listened_ms: 1,
  all_time_rank,
  new: isNew,
});

describe("statsModel", () => {
  it("steps the shorter way round the period ring", () => {
    expect(cyclesTo("30d", "all")).toEqual({ forward: false, count: 2 });
    expect(cyclesTo("7d", "30d")).toEqual({ forward: true, count: 1 });
    expect(cyclesTo("30d", "30d")).toEqual({ forward: true, count: 0 });
  });

  it("compares a rank with the all-time rank", () => {
    expect(moveText(artist(9), 4)).toEqual({ text: "▲ 5 from #9", tone: "up" });
    expect(moveText(artist(4), 7)).toEqual({
      text: "▼ 3 from #4",
      tone: "down",
    });
    expect(moveText(artist(1), 1)).toEqual({ text: "=", tone: "same" });
    expect(moveText(artist(12, true), 9)).toEqual({ text: "new", tone: "up" });
  });

  it("writes the movement with the period's own phrase", () => {
    expect(
      sentence({ name: "Alan Walker", kind: "climb", from: 9, to: 4 }, "30d"),
    ).toEqual([
      "Alan Walker climbed from #9 all time to ",
      "#4",
      " in the last 30 days.",
    ]);
    expect(
      sentence(
        { name: "Avicii", kind: "fall", from: 3, to: null },
        "month",
      ).join(""),
    ).toBe("Avicii, your #3 all time, has no plays this month.");
  });

  it("groups thousands and plays only Spotify tracks", () => {
    expect(groupThousands(1043)).toBe("1\u00a0043");
    expect(groupThousands(293)).toBe("293");
    expect(playable("spotify:track:1")).toBe(true);
    expect(playable("file:///a.flac")).toBe(false);
  });
});
