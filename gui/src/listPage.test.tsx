import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import type { LikedSongs } from "./bindings/LikedSongs";
import type { TrackInfo } from "./bindings/TrackInfo";
import type { TrackTablePayload } from "./bindings/TrackTablePayload";
import { ListPage } from "./ListPage";
import type { View } from "./nav";

const track = (name: string, duration_ms: number): TrackInfo => ({
  uri: `spotify:track:${name}`,
  name,
  artists: ["Adele"],
  album: "21",
  duration_ms,
  id: name,
  album_id: null,
  artist_refs: [],
  is_playable: true,
  is_local: false,
  track_number: 0,
  explicit: false,
  image_url: null,
  release_date: null,
});

const tracks = [
  track("Bravo", 2000),
  track("alpha", 3000),
  track("Charlie", 1000),
];

const liked: LikedSongs = {
  tracks,
  total: 3,
  has_more: false,
  available: true,
  loaded: true,
};

const mix: Extract<View, { page: "playlist" }> = {
  page: "playlist",
  uri: "spotify:playlist:mix",
  name: "Mix",
  source: "Spotify",
};

const table: TrackTablePayload = {
  uri: mix.uri,
  tracks,
  has_more: false,
  positions: [0, 1, 2],
  radio: null,
};

const page = (
  view: Extract<View, { page: "liked" | "playlist" }>,
  defaultSort?: string | null,
) =>
  renderToStaticMarkup(
    <ListPage
      view={view}
      liked={liked}
      table={table}
      playlists={null}
      source={null}
      playingUri={null}
      statusRev={null}
      statusError={false}
      active
      open={() => {}}
      send={() => {}}
      defaultSort={defaultSort}
    />,
  );

/** The row titles in the order the table draws them. */
const order = (html: string) =>
  [...html.matchAll(/class="title">([^<]+)</g)].map((match) => match[1]);

describe("ListPage sort and filter", () => {
  it("offers a Filter field and a Sort by select in the list actions", () => {
    const html = page({ page: "liked" });
    expect(html).toMatch(/<input type="search"[^>]*aria-label="Filter"/);
    expect(html).toMatch(/<select[^>]*aria-label="Sort by"/);
    expect(html).toMatch(/<option value="default" selected="">/);
    expect(html).toContain('<option value="duration:desc">');
  });

  it("keeps the loaded order without a default sort", () => {
    expect(order(page({ page: "liked" }))).toEqual([
      "Bravo",
      "alpha",
      "Charlie",
    ]);
    expect(order(page(mix, "rating"))).toEqual(["Bravo", "alpha", "Charlie"]);
  });

  it("opens a list in the default sort the setting names", () => {
    const html = page(mix, "name:desc");
    expect(order(html)).toEqual(["Charlie", "Bravo", "alpha"]);
    expect(html).toMatch(/<option value="name:desc" selected="">/);
    expect(order(page({ page: "liked" }, "duration"))).toEqual([
      "Charlie",
      "Bravo",
      "alpha",
    ]);
  });

  it("puts the cursor on the first shown row", () => {
    const html = page(mix, "name");
    expect(html).toMatch(/id="playlist-0"[^>]*aria-selected="true"/);
    expect(order(html)[0]).toBe("alpha");
  });

  it("shows no match count while nothing is filtered", () => {
    expect(page({ page: "liked" })).toContain(
      '<span class="list-count" aria-live="polite"></span>',
    );
  });
});

describe("ListPage reload", () => {
  it("shows the loading line, not the empty one, while a reload has no first page", () => {
    const html = renderToStaticMarkup(
      <ListPage
        view={mix}
        liked={liked}
        table={{ ...table, uri: null, tracks: [], positions: [] }}
        playlists={null}
        source={null}
        playingUri={null}
        statusRev={4}
        statusError={false}
        active
        open={() => {}}
        send={() => {}}
        defaultSort="name"
      />,
    );
    expect(html).toContain("Loading Mix…");
    expect(html).not.toContain("This playlist is empty.");
    expect(html).toMatch(/<option value="name" selected="">Title \(A-Z\)/);
    expect(html).toContain(
      '<span class="list-count" aria-live="polite"></span>',
    );
    expect(html).not.toContain("0 of 0");
  });
});

describe("ListPage sort labels", () => {
  it("names each sort in words", () => {
    const html = page({ page: "liked" });
    expect(html).toContain(
      '<option value="date_added:desc">Date added (newest first)</option>',
    );
    expect(html).toContain(
      '<option value="duration">Duration (shortest first)</option>',
    );
  });
});
