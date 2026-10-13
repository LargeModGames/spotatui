import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import type { DiscoverPayload } from "./bindings/DiscoverPayload";
import { Discover } from "./Discover";

const payload: DiscoverPayload = {
  available: true,
  loading: false,
  shelves: [
    {
      title: "Your Daily Mixes",
      playlists: [
        {
          uri: "spotify:playlist:1",
          name: "Daily Mix 1",
          description: "Kygo, Avicii and more",
          image_url: null,
        },
      ],
    },
    {
      title: "Discover New Music",
      playlists: [
        {
          uri: "spotify:playlist:2",
          name: "Release Radar",
          description: null,
          image_url: null,
        },
      ],
    },
  ],
};

const render = (discover: DiscoverPayload | null) =>
  renderToStaticMarkup(
    <Discover
      discover={discover}
      connected={false}
      active={false}
      open={() => {}}
      send={() => {}}
    />,
  );

describe("Discover", () => {
  it("draws one shelf per Made For You section with its playlists", () => {
    const html = render(payload);
    expect(html).toContain('aria-label="Your Daily Mixes"');
    expect(html).toContain('aria-label="Discover New Music"');
    expect(html).toContain("Play Daily Mix 1");
    expect(html).toContain("Kygo, Avicii and more");
    expect(html).toContain("By Spotify");
  });

  it("says it loads before the first shelves land", () => {
    expect(render({ ...payload, loading: true, shelves: null })).toContain(
      '<p class="empty" role="status">Loading…</p>',
    );
    expect(render({ ...payload, shelves: [] })).toContain("Nothing here yet.");
  });

  it("asks for a Spotify session without one", () => {
    expect(render({ ...payload, available: false })).toContain(
      "Discover needs a Spotify session.",
    );
  });
});
