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
  release_date: null,
});

const results: SearchPayload = {
  ran: true,
  query: "kygo",
  tracks: [
    track("Firestone", "1", "spotify:track:1"),
    track("Stay", "2", "spotify:track:2"),
    track("Stole the Show", "3", "spotify:track:3"),
    track("Carry Me", "4", "spotify:track:4"),
    track("Raging", "5", "spotify:track:5"),
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
  shows: [],
  liked: ["1"],
};

const render = (search: SearchPayload | null, waiting = false) =>
  renderToStaticMarkup(
    <Search
      search={search}
      waiting={waiting}
      source={{ active: "Spotify", compiled: ["Spotify", "Local", "YouTube"] }}
      playingUri="spotify:track:2"
      run={() => {}}
      query="kygo"
      open={() => {}}
      send={() => {}}
    />,
  );

describe("Search", () => {
  it("leads with the top artist and the first four songs, then the album shelf with its year", () => {
    const html = render(results);
    expect(html).toContain('aria-label="Top result"');
    expect(html).toContain('aria-label="Play Kygo"');
    expect(html).toMatch(/class="search-kind">Artist</);
    expect(html).toContain("search-3");
    expect(html).not.toContain("search-4");
    expect(html).toContain('<section class="shelf" aria-label="Albums">');
    expect(html).toContain("2016 · Kygo");
  });

  it("marks only the liked songs and the playing one", () => {
    const html = render(results);
    expect(html).toMatch(/id="search-0"[^]*?>Liked</);
    expect(html.match(/>Liked</g)).toHaveLength(1);
    expect(html).toMatch(/class="search-row now"[^]*?Playing: <\/span>Stay/);
  });

  it("offers the filter chips with All pressed and the empty kinds disabled", () => {
    const html = render(results);
    expect(html).toMatch(/aria-pressed="true">All</);
    expect(html).toMatch(/disabled="">Playlists</);
  });

  it("offers the other sources but not Local, which searches Spotify", () => {
    const html = render(results);
    expect(html).toContain("Search “kygo” in YouTube");
    expect(html).not.toContain("in Local");
  });

  it("uses one column when a source finds songs only", () => {
    const html = render({ ...results, artists: [], albums: [] });
    expect(html).toContain('class="search-lead"');
    expect(html).not.toContain("Top result");
  });

  it("asks for a search before one ran and says when one is running", () => {
    expect(render(null)).toContain(
      "Search for songs, artists, albums and playlists",
    );
    const waiting = render(null, true);
    expect(waiting).toContain("Searching…");
    expect(waiting).not.toContain("Search for songs");
  });

  it("shows a Podcasts shelf and chip for the shows it can open", () => {
    const show = {
      id: "s",
      uri: "spotify:show:s",
      name: "Lex Fridman Podcast",
      description: "",
      publisher: "Lex Fridman",
      image_url: null,
    };
    const html = render({
      ...results,
      shows: [show, { ...show, id: null, name: "No Id Show" }],
    });
    expect(html).toContain('<section class="shelf" aria-label="Podcasts">');
    expect(html).toContain("Lex Fridman Podcast");
    expect(html).toMatch(/class="card-subtitle">Lex Fridman</);
    expect(html).not.toContain("No Id Show");
    expect(html).not.toMatch(/disabled="">Podcasts</);
    expect(render(results)).toMatch(/disabled="">Podcasts</);
  });

  it("says when a search found nothing", () => {
    expect(
      render({ ...results, tracks: [], artists: [], albums: [] }),
    ).toContain("No results found for “kygo”.");
  });
});
