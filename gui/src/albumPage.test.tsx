import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import type { AlbumInfo } from "./bindings/AlbumInfo";
import type { TrackInfo } from "./bindings/TrackInfo";
import { AlbumPage } from "./AlbumPage";

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

const album: AlbumInfo = {
  id: "al",
  uri: "spotify:album:al",
  name: "Cloud Nine",
  artists: [
    { id: "ky", name: "Kygo" },
    { id: null, name: "Guest" },
  ],
  album_type: "single",
  release_date: "2016-05-13",
  total_tracks: 2,
  image_url: null,
  tracks: [track("Firestone"), track("Stay")],
};

const render = (id: string, value: AlbumInfo | null, saved = false) =>
  renderToStaticMarkup(
    <AlbumPage
      id={id}
      album={value}
      saved={saved}
      playingUri={null}
      open={() => {}}
      send={() => {}}
    />,
  );

describe("AlbumPage", () => {
  it("shows only a loading header before the album landed", () => {
    const html = render("al", null);
    expect(html).toContain("Loading…");
    expect(html).not.toContain('role="listbox"');
  });

  it("shows a loading header while another album is still loaded", () => {
    const html = render("other", album);
    expect(html).toContain("Loading…");
    expect(html).not.toContain("Firestone");
  });

  it("heads the page with the kind, the title and the meta line", () => {
    const html = render("al", album);
    expect(html).toContain(">Single<");
    expect(html).toContain("<h1>Cloud Nine</h1>");
    expect(html).toMatch(/<button[^>]*class="meta-link"[^>]*>Kygo<\/button>/);
    expect(html).not.toMatch(/<button[^>]*>Guest<\/button>/);
    expect(html).toContain("2016");
    expect(html).toContain("2 songs, 8 min");
  });

  it("offers play, save and the link, lists the tracks and ends on the release date", () => {
    const html = render("al", album);
    expect(html).toContain('aria-label="Play Cloud Nine"');
    expect(html).toMatch(
      /aria-label="Save to Your Library"[^>]*aria-pressed="false"/,
    );
    expect(html).toContain('aria-label="Copy album link"');
    expect(html).toContain('role="listbox"');
    expect(html).toContain("Firestone");
    expect(html).toContain("Stay");
    expect(html).toContain("May 13, 2016");
  });

  it("offers to remove a saved album with a pressed, filled heart", () => {
    const html = render("al", album, true);
    expect(html).toMatch(
      /class="icon-action on"[^>]*aria-label="Remove from Your Library"[^>]*aria-pressed="true"[^>]*><svg[^>]*fill="currentColor"/,
    );
    expect(html).not.toContain("Save to Your Library");
  });
});
