import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import type { TrackInfo } from "./bindings/TrackInfo";
import type { TrackTablePayload } from "./bindings/TrackTablePayload";
import { RadioPage } from "./RadioPage";

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

const table = (
  radio: string | null,
  tracks: TrackInfo[],
): TrackTablePayload => ({
  uri: null,
  tracks,
  has_more: false,
  positions: null,
  radio,
});

const render = (value: TrackTablePayload | null, available = true) =>
  renderToStaticMarkup(
    <RadioPage
      id="seed"
      name="Firestone"
      table={value}
      available={available}
      statusRev={null}
      statusError={false}
      playingUri={null}
      active
      open={() => {}}
      send={() => {}}
    />,
  );

describe("RadioPage", () => {
  it("heads the page with the radio name, its kind and its seed", () => {
    const html = render(table("seed", [track("Stay")]));
    expect(html).toContain('aria-label="Firestone Radio"');
    expect(html).toContain(">Song radio<");
    expect(html).toContain("<h1>Firestone Radio</h1>");
    expect(html).toContain("Based on Firestone");
    expect(html).toContain("1 song");
  });

  it("lists the rows and enables play once they are this radio's", () => {
    const html = render(table("seed", [track("Stay"), track("Carry Me")]));
    expect(html).toContain('role="listbox"');
    expect(html).toContain("Stay");
    expect(html).toContain("Carry Me");
    expect(html).not.toMatch(/aria-label="Play Firestone Radio"[^>]*disabled/);
  });

  it("shows a loading state while the table holds other rows", () => {
    for (const value of [
      null,
      table(null, [track("Playlist row")]),
      table("other", [track("Other radio row")]),
    ]) {
      const html = render(value);
      expect(html).toContain("Loading Firestone Radio…");
      expect(html).not.toContain('role="listbox"');
      expect(html).not.toContain("row<");
      expect(html).toMatch(/aria-label="Play Firestone Radio"[^>]*disabled/);
    }
  });

  it("says so when the radio has no rows", () => {
    const html = render(table("seed", []));
    expect(html).toContain("No songs like Firestone yet.");
    expect(html).not.toContain('role="listbox"');
  });
});

describe("RadioPage without Spotify", () => {
  it("says it needs a session instead of loading forever", () => {
    expect(render(null, false)).toContain(
      "Song radio needs a Spotify session.",
    );
  });
});
