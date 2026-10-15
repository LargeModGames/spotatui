import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import type { LyricsPayload } from "./bindings/LyricsPayload";
import type { NowPlaying } from "./bindings/NowPlaying";
import { LyricsPage } from "./LyricsPage";

const item: NowPlaying = {
  title: "Rolling in the Deep",
  artists: ["Adele"],
  album: "21",
  image_url: null,
  duration_ms: 240_000,
  uri: "spotify:track:t0",
  is_playing: false,
  is_live: false,
  shuffle: false,
  repeat: "off",
  context_uri: null,
  context_name: null,
  quality: null,
};

const lines = [
  { at_ms: 0, text: "There's a fire starting in my heart" },
  { at_ms: 4_000, text: "Reaching a fever pitch" },
  { at_ms: 8_000, text: "" },
];

const render = (lyrics: LyricsPayload | null, playing: NowPlaying | null) =>
  renderToStaticMarkup(
    <LyricsPage
      item={playing}
      lyrics={lyrics}
      position={{ ms: 5_000, at: 0 }}
      active={false}
      connected
      send={() => {}}
    />,
  );

describe("LyricsPage", () => {
  it("marks the sung line, the current line and the lines to come", () => {
    const html = render({ status: "found", synced: true, lines }, item);
    expect(html).toContain('<li class="past"><button');
    expect(html).toContain(
      '<li aria-current="true"><button type="button" class="lyric-seek" tabindex="0">',
    );
    expect(html).toContain('<li class="future"><button');
    expect(html).not.toContain("not synced");
  });

  it("puts the one tab stop on the sung line", () => {
    const html = render({ status: "found", synced: true, lines }, item);
    expect(html.match(/tabindex="0"/g)).toHaveLength(1);
    expect(html).toContain(
      '<li aria-current="true"><button type="button" class="lyric-seek" tabindex="0">Reaching a fever pitch</button>',
    );
  });

  it("shows unsynced lyrics plain with a note", () => {
    const html = render({ status: "found", synced: false, lines }, item);
    expect(html).toContain("These lyrics are not synced");
    expect(html).not.toContain("aria-current");
    expect(html).not.toContain("past");
    expect(html).not.toContain("<button");
  });

  it("says when nothing plays, lyrics load or none exist", () => {
    expect(render(null, null)).toContain("Nothing is playing.");
    expect(
      render({ status: "loading", synced: false, lines: [] }, item),
    ).toContain("Loading lyrics…");
    expect(
      render({ status: "not_found", synced: false, lines: [] }, item),
    ).toContain("No lyrics for this song");
  });
});
