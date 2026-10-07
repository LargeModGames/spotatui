import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import type { SearchPayload } from "./bindings/SearchPayload";
import type { TrackInfo } from "./bindings/TrackInfo";
import { Search } from "./Search";

const track = (name: string, id: string, uri: string): TrackInfo => ({
  uri,
  name,
  artists: ["Kygo"],
  album: "Cloud Nine",
  duration_ms: 272_000,
  id,
  album_id: null,
  artist_refs: [],
  is_playable: true,
  is_local: false,
  track_number: 0,
  explicit: false,
  image_url: null,
});

const results: SearchPayload = {
  ran: true,
  query: "kygo",
  tracks: [
    track("Firestone", "1", "spotify:track:1"),
    track("Stay", "2", "spotify:track:2"),
  ],
  artists: [
    { id: "k", uri: "spotify:artist:k", name: "Kygo", image_url: null },
  ],
  albums: [
    {
      id: "a",
      uri: "spotify:album:a",
      name: "Cloud Nine",
      artists: [{ id: "k", name: "Kygo" }],
      album_type: "album",
      release_date: "2016-05-13",
      total_tracks: null,
      image_url: null,
      tracks: [],
    },
  ],
  playlists: [],
  liked: ["1"],
};

const render = (search: SearchPayload | null) =>
  renderToStaticMarkup(
    <Search
      search={search}
      waiting={false}
      source={{ active: "Spotify", compiled: ["Spotify", "Local", "YouTube"] }}
      playingUri="spotify:track:2"
      query="kygo"
      onQuery={() => {}}
      onRun={() => {}}
      send={() => {}}
    />,
  );

describe("Search", () => {
  it("shows the top artist, the albums with their year and marks only the liked songs", () => {
    const html = render(results);
    expect(html).toContain("TOP RESULT · ARTIST");
    expect(html).toContain("Play Kygo");
    expect(html).toMatch(/disabled="">Open artist/);
    expect(html).toContain("Kygo · 2016");
    expect(html).toMatch(/id="search-0"[^]*?♥ liked/);
    expect(html).not.toContain(">new<");
    expect(html).toContain("▶ Stay");
  });

  it("offers the other sources but not Local, which searches Spotify", () => {
    const html = render(results);
    expect(html).toContain("Search “kygo” in YouTube");
    expect(html).not.toContain("in Local");
  });

  it("uses one column when a source finds songs only", () => {
    const html = render({ ...results, artists: [], albums: [] });
    expect(html).toContain('class="search-body"');
    expect(html).not.toContain("TOP RESULT");
  });

  it("asks for a search before one ran", () => {
    expect(render(null)).toContain("Type a search and press Enter.");
  });
});
