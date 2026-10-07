import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { CommandMode } from "./CommandMode";

const palette = (compiled: ("Spotify" | "YouTube")[]) =>
  renderToStaticMarkup(
    <CommandMode
      search={null}
      searchRev={1}
      source={{ active: "Spotify", compiled }}
      devices={[]}
      storage={{
        getItem: () => '["search john powell","play album 21"]',
        setItem: () => {},
      }}
      send={() => {}}
      onSearch={() => {}}
      onClose={() => {}}
    />,
  );

describe("CommandMode", () => {
  it("lists the recent commands and the commands that run in this build", () => {
    const html = palette(["Spotify"]);
    expect(html).toContain("search john powell");
    expect(html).toContain("<b>party</b> <span>listening party</span>");
    expect(html).not.toContain("<b>source</b>");
    expect(palette(["Spotify", "YouTube"])).toContain("<b>source</b>");
  });

  it("asks for a command before anything is typed", () => {
    expect(palette(["Spotify"])).toContain(
      "Type a command, or words to search",
    );
  });
});
