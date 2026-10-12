import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import type { AlbumInfo } from "./bindings/AlbumInfo";
import type { ArtistPayload } from "./bindings/ArtistPayload";
import type { TrackInfo } from "./bindings/TrackInfo";
import { ArtistPage } from "./ArtistPage";

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

const album = (name: string, type: string, date: string | null): AlbumInfo => ({
  id: name,
  uri: `spotify:album:${name}`,
  name,
  artists: [],
  album_type: type,
  release_date: date,
  total_tracks: 10,
  image_url: null,
  tracks: [],
});

const payload = (topCount: number, followed: boolean): ArtistPayload => ({
  page: {
    id: "ky",
    name: "Kygo",
    image_url: "https://i.scdn.co/image/kygo",
    following: followed,
    top_tracks: Array.from({ length: topCount }, (_, index) =>
      track(`Hit${index}`),
    ),
    albums: [
      album("Cloud Nine", "album", "2016-05-13"),
      album("Stay", "single", null),
    ],
    related_artists: [
      { id: "av", uri: null, name: "Avicii", image_url: null },
      { id: null, uri: null, name: "Nobody", image_url: null },
    ],
  },
});

const render = (id: string, artist: ArtistPayload | null) =>
  renderToStaticMarkup(
    <ArtistPage
      id={id}
      name="Kygo"
      artist={artist}
      playingUri={null}
      open={() => {}}
      send={() => {}}
    />,
  );

describe("ArtistPage", () => {
  it("shows a loading header with the name before the artist landed", () => {
    const html = render("ky", null);
    expect(html).toContain("<h1>Kygo</h1>");
    expect(html).not.toContain("Popular");
  });

  it("shows a loading header while another artist is still loaded", () => {
    expect(render("other", payload(3, false))).not.toContain("Popular");
  });

  it("shows the artist's portrait in the header", () => {
    expect(render("ky", payload(3, false))).toContain(
      'src="https://i.scdn.co/image/kygo"',
    );
  });

  it("offers play and follow, and says Following for a followed artist", () => {
    expect(render("ky", payload(3, false))).toMatch(
      /aria-pressed="false"[^>]*>Follow</,
    );
    const html = render("ky", payload(3, true));
    expect(html).toContain('aria-label="Play Kygo"');
    expect(html).toMatch(/aria-pressed="true"[^>]*>Following</);
  });

  it("lists five popular tracks with a See more button", () => {
    const html = render("ky", payload(8, false));
    expect(html).toContain("Hit4");
    expect(html).not.toContain("Hit5");
    expect(html).toContain("See more");
  });

  it("drops See more when there are five tracks or fewer", () => {
    expect(render("ky", payload(5, false))).not.toContain("See more");
  });

  it("shows the discography and the related artists that have an id", () => {
    const html = render("ky", payload(3, false));
    expect(html).toContain('aria-label="Discography"');
    expect(html).toContain("2016 · Album");
    expect(html).toContain(">Single<");
    expect(html).toContain('aria-label="Fans also like"');
    expect(html).toContain("Avicii");
    expect(html).not.toContain("Nobody");
  });
});
