import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import type { TrackInfo } from "./bindings/TrackInfo";
import { RecentlyPlayed } from "./RecentlyPlayed";

const track = (name: string): TrackInfo => ({
  uri: `spotify:track:${name}`,
  name,
  artists: ["Kygo"],
  album: "Cloud Nine",
  duration_ms: 240_000,
  id: name,
  album_id: "al",
  artist_refs: [],
  is_playable: true,
  is_local: false,
  track_number: 0,
  explicit: false,
  image_url: null,
  release_date: null,
});

const render = (recent: TrackInfo[] | null, available = true) =>
  renderToStaticMarkup(
    <RecentlyPlayed
      recent={recent}
      available={available}
      playingUri="spotify:track:Stay"
      active
      open={() => {}}
      send={() => {}}
    />,
  );

describe("RecentlyPlayed", () => {
  it("heads the page and counts the plays", () => {
    const html = render([track("Stay"), track("Firestone")]);
    expect(html).toContain("<h1>Recently played</h1>");
    expect(html).toContain("2 songs");
    expect(html).toContain('aria-label="Play Recently played"');
  });

  it("keeps a track played twice as two rows", () => {
    const html = render([track("Stay"), track("Firestone"), track("Stay")]);
    expect(html).toContain('id="recent-0"');
    expect(html).toContain('id="recent-2"');
    expect(html.split('class="track-row now"').length).toBe(3);
  });

  it("says it is loading before the first fetch", () => {
    const html = render(null);
    expect(html).toContain("Loading recently played…");
    expect(html).not.toContain('role="listbox"');
    expect(html).toMatch(/aria-label="Play Recently played"[^>]*disabled/);
  });

  it("says so when nothing was played", () => {
    const html = render([]);
    expect(html).toContain("Nothing played yet.");
    expect(html).not.toContain('role="listbox"');
  });
});

describe("RecentlyPlayed without Spotify", () => {
  it("says it needs a session instead of loading forever", () => {
    expect(render(null, false)).toContain(
      "Recently played needs a Spotify session.",
    );
  });
});
