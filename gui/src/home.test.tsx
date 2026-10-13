import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import type { DiscoverPayload } from "./bindings/DiscoverPayload";
import type { PlaylistInfo } from "./bindings/PlaylistInfo";
import type { SourcePlaylists } from "./bindings/SourcePlaylists";
import type { TrackInfo } from "./bindings/TrackInfo";
import { Home } from "./Home";

const track = (name: string, album_id: string | null): TrackInfo => ({
  uri: `spotify:track:${name}`,
  name,
  artists: ["Kygo"],
  album: `Album ${album_id}`,
  duration_ms: 200_000,
  id: name,
  album_id,
  artist_refs: [],
  is_playable: true,
  is_local: false,
  track_number: 0,
  explicit: false,
  image_url: null,
  release_date: null,
});

const playlist = (name: string): PlaylistInfo => ({
  uri: `spotify:playlist:${name}`,
  name,
  owner: "jay",
  track_count: 3,
  id: name,
  owner_id: "jay",
  collaborative: false,
  public: true,
  image_url: null,
});

const library = (count: number): SourcePlaylists => ({
  spotify: Array.from({ length: count }, (_, index) => playlist(`P${index}`)),
  local: [],
  subsonic: [],
  qobuz: [],
  youtube: [],
  radio: [],
  saved_albums: [],
  saved_albums_has_more: false,
  followed_artists: [],
  saved_shows: [],
  saved_shows_has_more: false,
  saved_show_ids: [],
  folders: null,
  editable: [],
});

const discover: DiscoverPayload = {
  available: true,
  loading: false,
  shelves: [
    {
      title: "Uniquely yours",
      playlists: [
        {
          uri: "spotify:playlist:daylist",
          name: "daylist",
          description: "Your day in a playlist.",
          image_url: null,
        },
      ],
    },
    { title: "Your Daily Mixes", playlists: [] },
  ],
};

const render = (
  recent: TrackInfo[] | null,
  playlists: SourcePlaylists | null,
  disc: DiscoverPayload | null,
) =>
  renderToStaticMarkup(
    <Home
      recent={recent}
      playlists={playlists}
      discover={disc}
      connected={false}
      active={false}
      playingUri={null}
      open={() => {}}
      send={() => {}}
    />,
  );

describe("Home", () => {
  it("greets by the time of day", () => {
    expect(render(null, null, null)).toMatch(
      /<h1>Good (morning|afternoon|evening)<\/h1>/,
    );
  });

  it("puts Liked Songs first in the quick grid and gives it the focus", () => {
    const html = render(null, library(2), null);
    expect(html).toMatch(/data-focus="true"[^>]*>.*?Liked Songs/);
    expect(html.indexOf("Liked Songs")).toBeLessThan(html.indexOf("P0"));
  });

  it("shows at most seven playlists in the quick grid", () => {
    const html = render(null, library(10), null);
    const grid = html.slice(0, html.indexOf('aria-label="Your playlists"'));
    expect(grid).toContain("Play P6");
    expect(grid).not.toContain("Play P7");
  });

  it("shows one recently played card per album", () => {
    const html = render(
      [track("One", "a"), track("Two", "a"), track("Three", "b")],
      null,
      null,
    );
    expect(html).toContain('aria-label="Recently played"');
    expect(html).toContain("Album a");
    expect(html).toContain("Album b");
    expect(html.split("Play Album a").length).toBe(2);
  });

  it("offers Show all on the recently played and Made For You shelves only", () => {
    const html = render([track("One", "a")], library(1), discover);
    expect(html).toMatch(
      /<button[^>]*class="shelf-more"[^>]*aria-label="Show all: Recently played"[^>]*>Show all<\/button>/,
    );
    expect(html).toContain('aria-label="Show all: Made For You"');
    expect(html.split('class="shelf-more"').length).toBe(3);
  });

  it("draws Made For You from every Discover shelf and skips empty shelves", () => {
    const html = render(null, null, discover);
    expect(html).toContain('aria-label="Made For You"');
    expect(html).toContain("Play daylist");
    expect(html).toContain("Your day in a playlist.");
    expect(html).not.toContain('aria-label="Saved albums"');
    expect(html).not.toContain('aria-label="Recently played"');
  });
});
