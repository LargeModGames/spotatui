import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import type { DiscoverPayload } from "./bindings/DiscoverPayload";
import type { TrackInfo } from "./bindings/TrackInfo";
import { Discover } from "./Discover";

const track = (name: string, id: string): TrackInfo => ({
  uri: `spotify:track:${id}`,
  name,
  artists: ["Kygo"],
  album: "Cloud Nine",
  duration_ms: 240_000,
  id,
  album_id: null,
  artist_refs: [],
  is_playable: true,
  is_local: false,
  track_number: 0,
  explicit: false,
  image_url: null,
  release_date: null,
});

const payload: DiscoverPayload = {
  available: true,
  loading: false,
  top_tracks_range: "Medium",
  top_tracks: [track("Firestone", "1"), track("Stay", "2")],
  artists_mix: [],
  artists_mix_available: true,
  liked_ids: ["1"],
};

const render = (discover: DiscoverPayload | null) =>
  renderToStaticMarkup(
    <Discover
      discover={discover}
      active={false}
      playingUri={null}
      send={() => {}}
    />,
  );

describe("Discover", () => {
  it("lists the top tracks of the range with a liked mark and no unliked mark", () => {
    const html = render(payload);
    expect(html).toContain("Your top tracks, 6 months");
    expect(html).toMatch(/id="discover-0"[^]*?♥ liked/);
    expect(html).not.toContain("not liked");
    expect(html).toContain("Add 2 to queue");
  });

  it("keeps the status outside the list, which holds only options", () => {
    const html = render({ ...payload, top_tracks: [] });
    expect(html).toContain(
      '<p class="empty" role="status">Nothing here yet.</p>',
    );
    expect(html).toMatch(/role="listbox"[^>]*><\/div>/);
  });

  it("asks for a Spotify session without one", () => {
    expect(render({ ...payload, available: false })).toContain(
      "Discover needs a Spotify session.",
    );
  });
});
