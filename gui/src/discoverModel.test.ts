import { describe, expect, it } from "vitest";
import type { DiscoverPayload } from "./bindings/DiscoverPayload";
import { fetchFor, rowsFor, stepRange } from "./discoverModel";

const discover = (patch: Partial<DiscoverPayload> = {}): DiscoverPayload => ({
  available: true,
  loading: false,
  top_tracks_range: "Medium",
  top_tracks: [],
  artists_mix: [],
  artists_mix_available: true,
  liked_ids: [],
  ...patch,
});

describe("discoverModel", () => {
  it("fetches a range only when the cache holds another and it was not asked for", () => {
    expect(fetchFor(discover(), "top", "Short", null)).toEqual({
      OpenDiscover: { TopTracks: "Short" },
    });
    expect(fetchFor(discover(), "top", "Medium", null)).toBeNull();
    expect(fetchFor(discover(), "top", "Short", "Short")).toBeNull();
    expect(
      fetchFor(discover({ loading: true }), "top", "Short", null),
    ).toBeNull();
    expect(
      fetchFor(discover({ available: false }), "top", "Short", null),
    ).toBeNull();
  });

  it("fetches the mix once when the key allows it", () => {
    expect(fetchFor(discover(), "mix", "Medium", null)).toEqual({
      OpenDiscover: "ArtistsMix",
    });
    expect(fetchFor(discover(), "mix", "Medium", "mix")).toBeNull();
    expect(
      fetchFor(
        discover({ artists_mix_available: false }),
        "mix",
        "Medium",
        null,
      ),
    ).toBeNull();
  });

  it("shows top tracks only under the range they belong to", () => {
    const loaded = discover({
      top_tracks: [
        {
          uri: "spotify:track:1",
          name: "Freeze",
          artists: [],
          album: "",
          duration_ms: 1,
          id: "1",
          album_id: null,
          artist_refs: [],
          is_playable: true,
          is_local: false,
          track_number: 0,
          explicit: false,
          image_url: null,
          release_date: null,
        },
      ],
    });
    expect(rowsFor(loaded, "top", "Medium")).toHaveLength(1);
    expect(rowsFor(loaded, "top", "Short")).toHaveLength(0);
  });

  it("steps round the ranges", () => {
    expect(stepRange("Long", true)).toBe("Short");
    expect(stepRange("Short", false)).toBe("Long");
  });
});
