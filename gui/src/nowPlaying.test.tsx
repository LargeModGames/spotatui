import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import type { NowPlaying } from "./bindings/NowPlaying";
import type { TrackInfo } from "./bindings/TrackInfo";
import { NowPlayingPanel } from "./NowPlayingPanel";

const song: NowPlaying = {
  title: "Set Fire to the Rain",
  artists: ["Adele"],
  album: "21",
  image_url: "https://img/21.jpg",
  duration_ms: 243_000,
  uri: "spotify:track:1",
  is_playing: true,
  is_live: false,
  shuffle: false,
  repeat: "off",
  context_uri: null,
  context_name: null,
  quality: null,
};

const next: TrackInfo = {
  uri: "spotify:track:2",
  name: "Lovesong",
  artists: ["Adele", "Guest"],
  album: "21",
  duration_ms: 316_000,
  id: "2",
  album_id: null,
  artist_refs: [
    { id: "adele", name: "Adele" },
    { id: null, name: "Guest" },
  ],
  is_playable: true,
  is_local: false,
  track_number: 10,
  explicit: false,
  image_url: null,
  release_date: null,
};

const panel = (
  item: NowPlaying | null,
  upNext: TrackInfo[] = [],
  liked = false,
) =>
  renderToStaticMarkup(
    <NowPlayingPanel
      playback={{ item, volume: 50, device: "This PC", liked }}
      upNext={upNext}
      open={() => {}}
      send={() => {}}
      onClose={() => {}}
    />,
  );

describe("NowPlayingPanel", () => {
  it("shows the playing track large with its artists and a close button", () => {
    const html = panel(song);
    expect(html).toContain(">Now playing<");
    expect(html).toContain('aria-label="Close now playing view"');
    expect(html).toContain('src="https://img/21.jpg"');
    expect(html).toContain(">Set Fire to the Rain<");
    expect(html).toContain(">Adele<");
  });

  it("shows the stream quality after the artists only when there is one", () => {
    const html = panel({ ...song, quality: "FLAC 16/44.1" });
    expect(html).toContain('<span class="quality"> • FLAC 16/44.1</span>');
    expect(panel(song)).not.toContain('class="quality"');
  });

  it("names the album when the album is the play context", () => {
    const html = panel({ ...song, context_uri: "spotify:album:21" });
    expect(html).toMatch(/<h2>21<\/h2>/);
  });

  it("offers the heart for a Spotify track only, pressed when it is liked", () => {
    expect(panel(song, [], true)).toMatch(
      /aria-label="Like" aria-pressed="true"/,
    );
    expect(panel({ ...song, uri: "file:///21/05.flac" })).not.toContain(
      'aria-label="Like"',
    );
  });

  it("shows the next track and links only the artists with an id", () => {
    const html = panel(song, [next]);
    expect(html).toContain('aria-label="Next in queue"');
    expect(html).toContain(">Lovesong<");
    expect(html).toMatch(
      /<button[^>]*class="artist-link"[^>]*>Adele<\/button>/,
    );
    expect(html).not.toMatch(/<button[^>]*>Guest<\/button>/);
    expect(html).toContain("Guest");
  });

  it("leaves out the next card when nothing is queued", () => {
    expect(panel(song)).not.toContain("Next in queue");
  });

  it("says nothing is playing without an item", () => {
    expect(panel(null)).toContain("Nothing is playing.");
  });
});
