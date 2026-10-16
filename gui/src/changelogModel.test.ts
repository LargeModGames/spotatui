import { describe, expect, it } from "vitest";
import { parseBlocks, parseInline, releases } from "./changelogModel";

describe("releases", () => {
  it("splits on release headings and drops the title above the first", () => {
    const text =
      "# Changelog\n\n## [Unreleased]\n\n- a\n\n## [v1.0.0] 2026-01-01\n\n- b\n";
    expect(releases(text)).toEqual([
      "## [Unreleased]\n\n- a\n",
      "## [v1.0.0] 2026-01-01\n\n- b\n",
    ]);
  });
});

describe("parseInline", () => {
  it("reads bold, inline code and https links", () => {
    expect(
      parseInline(
        "**Search** with `Ctrl+F` ([#840](https://github.com/x/issues/840)).",
      ),
    ).toEqual([
      { text: "Search", bold: true },
      { text: " with " },
      { text: "Ctrl+F", code: true },
      { text: " (" },
      { text: "#840", url: "https://github.com/x/issues/840" },
      { text: ")." },
    ]);
  });

  it("keeps a link inside bold text bold", () => {
    expect(parseInline("**see [docs](https://a.b/c)**")).toEqual([
      { text: "see ", bold: true },
      { text: "docs", bold: true, url: "https://a.b/c" },
    ]);
  });

  it("links a bare https URL without its closing punctuation", () => {
    expect(parseInline("Join us: https://spotatui.com/discord.")).toEqual([
      { text: "Join us: " },
      {
        text: "https://spotatui.com/discord",
        url: "https://spotatui.com/discord",
      },
      { text: "." },
    ]);
  });

  it("shows a relative or non-https target as its label only", () => {
    expect(parseInline("[setup](docs/mcp-setup.md) and `http://x`")).toEqual([
      { text: "setup and " },
      { text: "http://x", code: true },
    ]);
    expect(parseInline("[x](javascript:alert(1))")).toEqual([{ text: "x)" }]);
  });

  it("leaves raw HTML as text", () => {
    expect(parseInline("<b>hi</b>")).toEqual([{ text: "<b>hi</b>" }]);
  });
});

describe("parseBlocks", () => {
  it("reads headings, nested bullets, wrapped lines and paragraphs", () => {
    const blocks = parseBlocks(
      "## [v1] 2026-01-01\n\n### Added\n\n- **One**\n  more\n  - nested\n\nA note.\n",
    );
    expect(blocks).toEqual([
      { kind: "heading", level: 2, spans: [{ text: "[v1] 2026-01-01" }] },
      { kind: "heading", level: 3, spans: [{ text: "Added" }] },
      {
        kind: "item",
        depth: 0,
        spans: [{ text: "One", bold: true }, { text: " more" }],
      },
      { kind: "item", depth: 1, spans: [{ text: "nested" }] },
      { kind: "paragraph", spans: [{ text: "A note." }] },
    ]);
  });
});
