import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { STYLES } from "./screenModel";
import { styleName, VisualizerScreen } from "./VisualizerScreen";
import { SCREENS_EN } from "./i18n";

const screen = (available: boolean) =>
  renderToStaticMarkup(
    <VisualizerScreen
      item={null}
      position={null}
      active
      available={available}
    />,
  );

describe("VisualizerScreen", () => {
  it("offers all nine terminal styles by name, Cava first", () => {
    const html = screen(true);
    const names = STYLES.map((style) => styleName(style, SCREENS_EN));
    expect(names).toEqual([
      "Cava",
      "Bar Graph",
      "Head Unit",
      "Analog VU",
      "Winamp",
      "Phosphor Scope",
      "Outrun",
      "Rack EQ",
      "Cassette Deck",
    ]);
    names.forEach((name) => expect(html).toContain(`>${name}</option>`));
    expect(html).toContain('<option value="Cava" selected="">');
    expect(html).toContain('aria-keyshortcuts="V"');
  });

  it("draws on a canvas only where the build can capture", () => {
    expect(screen(true)).toContain('class="viz-screen-canvas"');
    const without = screen(false);
    expect(without).not.toContain("<canvas");
    expect(without).toContain("No audio capture available");
  });
});
