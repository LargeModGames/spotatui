import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import type { Source } from "./bindings/Source";
import { SourceBadge } from "./SourceBadge";

const SOURCES: Source[] = [
  "Spotify",
  "Local",
  "Subsonic",
  "Radio",
  "YouTube",
  "Qobuz",
  "AppleMusic",
];

describe("SourceBadge", () => {
  it.each(SOURCES)("draws a hidden icon before the %s label", (source) => {
    const html = renderToStaticMarkup(<SourceBadge source={source} />);
    expect(html).toMatch(
      new RegExp(
        `<svg class="source-icon ${source.toLowerCase()}"[^>]*aria-hidden="true" focusable="false"><(path|g) `,
      ),
    );
    expect(html).toContain(`<span class="name">${source.toUpperCase()}</span>`);
  });
});
