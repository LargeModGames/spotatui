import { describe, expect, it } from "vitest";
import type { SyncLinkView } from "./bindings/SyncLinkView";
import {
  cellFor,
  lastSync,
  reasonText,
  selectedIndex,
  unmatchedTotal,
} from "./healthModel";

const link: SyncLinkView = {
  id: "a",
  source: "Spotify",
  name: "Road Trip",
  last_line: null,
  last_failed: false,
  mirrors: [
    {
      source: "Qobuz",
      matched: 40,
      unmatched: [
        {
          master_key: "k",
          title: "Levels",
          artist: "Avicii",
          reason: "NoCandidate",
        },
      ],
      last_run: "2026-09-29T12:00:00Z",
    },
    { source: "Subsonic", matched: 0, unmatched: [], last_run: null },
    {
      source: "YouTube",
      matched: 41,
      unmatched: [],
      last_run: "2026-09-28T12:00:00Z",
    },
  ],
};

describe("healthModel", () => {
  it("names the master, each mirror's state and the sources without a copy", () => {
    expect(cellFor(link, "Spotify").role).toBe("master");
    expect(cellFor(link, "Qobuz")).toEqual({ role: "unmatched", count: 1 });
    expect(cellFor(link, "Subsonic").role).toBe("never");
    expect(cellFor(link, "YouTube").role).toBe("synced");
    expect(cellFor({ ...link, mirrors: [] }, "Qobuz").role).toBe("none");
  });

  it("adds up the unmatched tracks and words the reasons as the terminal does", () => {
    expect(unmatchedTotal([link, link])).toBe(2);
    expect(reasonText("NotSyncable")).toBe("cannot sync");
    expect(reasonText({ SearchFailed: "429" })).toBe("search failed: 429");
  });

  it("takes the newest mirror run, and never without one", () => {
    expect(lastSync(link)).toMatch(/^2026-09-29 \d\d:00$/);
    expect(lastSync({ ...link, mirrors: [] })).toBe("never");
  });

  it("keeps the selection by id and falls back to the first link", () => {
    expect(selectedIndex([link], "gone")).toBe(0);
    expect(selectedIndex([], null)).toBe(-1);
  });
});
