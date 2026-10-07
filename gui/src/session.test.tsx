import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import type { SessionPlay } from "./bindings/SessionPlay";
import { Session } from "./Session";

const play = (title: string, uri: string, start: number): SessionPlay => ({
  started_at_ms: start,
  ended_at_ms: start + 240_000,
  listened_ms: 240_000,
  duration_ms: 240_000,
  title,
  artists: ["Adele"],
  album: "21",
  uri,
  image_url: null,
});

const render = (plays: SessionPlay[]) =>
  renderToStaticMarkup(
    <Session
      plays={plays}
      item={null}
      position={null}
      queue={null}
      upNext={[]}
      active={false}
      send={() => {}}
    />,
  );

describe("Session", () => {
  it("lists the earlier plays newest first with their source and marks a source change", () => {
    const html = render([
      play("Firework", "spotify:track:1", 0),
      play("Rolling in the Deep", "file:///21/01.flac", 240_000),
    ]);
    expect(html.indexOf("Rolling in the Deep")).toBeLessThan(
      html.lastIndexOf("Firework"),
    );
    expect(html).toContain('class="swatch local"');
    expect(html).toContain("SPOTIFY → LOCAL");
    expect(html).toContain("2 songs · 2 sources");
  });

  it("says nothing has finished before the first play", () => {
    expect(render([])).toContain("Nothing has finished playing yet.");
  });
});
