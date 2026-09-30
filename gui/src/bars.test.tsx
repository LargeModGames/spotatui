import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import type { NowPlaying } from "./bindings/NowPlaying";
import { PlayerBar } from "./PlayerBar";
import { TopBar } from "./TopBar";

const song: NowPlaying = {
  title: "Set Fire to the Rain",
  artists: ["Adele"],
  album: "21",
  image_url: null,
  duration_ms: 243_000,
  uri: "file:///music/21/05.flac",
  is_playing: false,
  is_live: false,
  shuffle: false,
  repeat: "off",
};

const player = (item: NowPlaying | null) =>
  renderToStaticMarkup(
    <PlayerBar
      playback={{ item, volume: 72, device: "This PC", liked: false }}
      position={{ ms: 100_000, at: 0 }}
      connected
      send={() => {}}
    />,
  );

describe("PlayerBar", () => {
  it("shows the source, the position and the length of a paused song", () => {
    const html = player(song);
    expect(html).toContain("Adele · 21");
    expect(html).toContain("LOCAL");
    expect(html).toContain(">1:40<");
    expect(html).toContain(">4:03<");
    expect(html).toContain('aria-label="Play"');
  });

  it("exposes seek and volume as focusable sliders", () => {
    const html = player(song);
    expect(html).toMatch(/role="slider" tabindex="0" aria-label="Seek"/);
    expect(html).toContain('aria-valuetext="1:40 of 4:03"');
    expect(html).toMatch(/role="slider" tabindex="0" aria-label="Volume"/);
    expect(html).toContain('aria-valuetext="72%"');
  });

  it("marks shuffle and repeat one as pressed", () => {
    const html = player({ ...song, shuffle: true, repeat: "track" });
    expect(html).toContain('aria-label="Shuffle" aria-pressed="true"');
    expect(html).toContain('aria-label="Repeat one" aria-pressed="true"');
  });

  it("counts the time of a stream up and shows it as live", () => {
    const html = player({ ...song, is_live: true, duration_ms: 0 });
    expect(html).toContain(">1:40<");
    expect(html).toContain(">live<");
  });

  it("shows nothing playing without an item", () => {
    expect(player(null)).toContain("Nothing playing");
  });
});

describe("TopBar", () => {
  const bar = (device: string | null, connected: boolean) =>
    renderToStaticMarkup(
      <TopBar
        area="library"
        ready={(area) => area === "library"}
        onArea={() => {}}
        query=""
        onSearch={() => {}}
        device={device}
        connected={connected}
        queued={6}
        queueOpen={false}
        onQueue={() => {}}
      />,
    );

  it("marks the active area and disables the areas with no screen yet", () => {
    const html = bar("This PC", true);
    expect(html).toMatch(/aria-current="page"[^>]*>Library<kbd>1</);
    expect(html).toMatch(/disabled=""[^>]*>Search<kbd>2</);
  });

  it("lights the device dot only for a connected device", () => {
    expect(bar("This PC", true)).toContain('class="dot on"');
    expect(bar(null, true)).toContain("No device");
    expect(bar(null, true)).not.toContain('class="dot on"');
    expect(bar("This PC", false)).not.toContain('class="dot on"');
  });
});
