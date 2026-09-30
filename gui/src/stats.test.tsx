import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import type { StatsPayload } from "./bindings/StatsPayload";
import type { StatsRow } from "./bindings/StatsRow";
import { Stats } from "./Stats";

const row = (title: string, patch: Partial<StatsRow> = {}): StatsRow => ({
  title,
  artist: null,
  uri: null,
  listened_ms: 1,
  all_time_rank: null,
  new: false,
  ...patch,
});

const loaded: StatsPayload = {
  period: "30d",
  loading: false,
  loaded: true,
  plays: [
    { period: "7d", plays: 293 },
    { period: "30d", plays: 1043 },
    { period: "month", plays: 900 },
    { period: "year", plays: 3000 },
    { period: "all", plays: 4324 },
  ],
  top_artists: [
    row("Kygo", { all_time_rank: 1 }),
    row("Alan Walker", { all_time_rank: 9 }),
  ],
  top_albums: [row("Cloud Nine")],
  top_tracks: [row("Freeze", { artist: "Kygo", uri: "spotify:track:1" })],
  week_tracks: [row("Levels", { artist: "Avicii" })],
  movements: [{ name: "Alan Walker", kind: "climb", from: 9, to: 2 }],
};

const render = (stats: StatsPayload | null) =>
  renderToStaticMarkup(<Stats stats={stats} send={() => {}} />);

describe("Stats", () => {
  it("shows every period's plays with the selected one marked", () => {
    const html = render(loaded);
    expect(html).toContain('class="big current">1\u00a0043<');
    expect(html).toContain("plays, all time");
    expect(html).toMatch(/aria-selected="true"[^>]*>30 days</);
  });

  it("compares each artist with its all-time rank and writes the movement", () => {
    const html = render(loaded);
    expect(html).toContain('class="move same">=<');
    expect(html).toContain('class="move up">▲ 7 from #9<');
    expect(html).toContain("<em>#2</em> in the last 30 days.");
    expect(html).toContain("<b>Freeze</b> <span>Kygo</span>");
  });

  it("hides the all-time comparison when all time is the period", () => {
    expect(render({ ...loaded, period: "all" })).not.toContain("VS ALL TIME");
  });

  it("says when there is no history or nothing is loaded yet", () => {
    const none = loaded.plays.map((entry) => ({ ...entry, plays: 0 }));
    expect(render({ ...loaded, plays: none })).toContain(
      "No listening history recorded yet.",
    );
    expect(render({ ...loaded, loaded: false, loading: true })).toContain(
      "Loading…",
    );
  });
});
